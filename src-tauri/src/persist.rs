//! The `.lawha` file format: one SQLite database per document.
//!
//! - `meta(key, value)`: format version, canvas size, resolution, active layer, name.
//! - `nodes(id, parent, idx, kind, props)`: the layer tree (props as JSON).
//! - `tiles(node, tx, ty, hash)`: which tile goes where.
//! - `blobs(hash, data)`: tile pixels, LZ4-compressed, stored once per distinct content (BLAKE3).
//!
//! Saving into an existing file is incremental: only tiles whose content is new are written, inside
//! one SQLite transaction, so a crash mid-save leaves the previous version intact. Autosave uses the
//! same code on a recovery file.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Weak};

use lawha_core::tile::{Tile, TILE_BYTES};
use lawha_core::BlendMode;
use parking_lot::Mutex;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::doc::model::{LayerMask, Locks, Node, NodeKind, TileMap};
use lawha_core::mask::{Mask, MaskMap, MaskTile, MASK_TILE_BYTES};
use crate::doc::{reserve_ids_above, Content};

pub const FORMAT: &str = "lawha/1";

type Hash = [u8; 32];

/// Remembers the hash of tiles already seen, keyed by allocation, so unchanged tiles are never
/// hashed or compressed twice.
#[derive(Default)]
pub struct HashCache {
    map: Mutex<HashMap<usize, (Weak<Tile>, Hash)>>,
}

impl HashCache {
    fn hash(&self, tile: &Arc<Tile>) -> Hash {
        let key = Arc::as_ptr(tile) as usize;
        if let Some((w, h)) = self.map.lock().get(&key) {
            if w.upgrade().is_some_and(|t| Arc::ptr_eq(&t, tile)) {
                return *h;
            }
        }
        let h = *blake3::hash(&tile.0).as_bytes();
        let mut m = self.map.lock();
        if m.len() > 200_000 {
            m.retain(|_, (w, _)| w.strong_count() > 0);
        }
        m.insert(key, (Arc::downgrade(tile), h));
        h
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Props {
    name: String,
    visible: bool,
    opacity: f32,
    fill: f32,
    blend: BlendMode,
    locks: Locks,
    clipped: bool,
    #[serde(default)]
    pass_through: bool,
    #[serde(default)]
    expanded: bool,
    /// Layer mask settings; its pixels are in the `masks` table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mask: Option<MaskProps>,
    /// Settings of an adjustment layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    adjustment: Option<lawha_core::adjust::Adjustment>,
    /// A text layer's text, style and placement (its last drawing is in the tiles table).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<crate::doc::text::TextData>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MaskProps {
    enabled: bool,
    density: f32,
    feather: f32,
    outside: u8,
}

#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error("not a Lawha document")]
    NotLawha,
    #[error("this file was made by a newer version of Lawha ({0})")]
    TooNew(String),
    #[error("damaged file: {0}")]
    Damaged(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS nodes (id INTEGER PRIMARY KEY, parent INTEGER, idx INTEGER NOT NULL, kind TEXT NOT NULL, props TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS tiles (node INTEGER NOT NULL, tx INTEGER NOT NULL, ty INTEGER NOT NULL, hash BLOB NOT NULL, PRIMARY KEY (node, tx, ty));
CREATE TABLE IF NOT EXISTS blobs (hash BLOB PRIMARY KEY, data BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS masks (node INTEGER NOT NULL, tx INTEGER NOT NULL, ty INTEGER NOT NULL, hash BLOB NOT NULL, PRIMARY KEY (node, tx, ty));
";

fn open_rw(path: &Path) -> Result<Connection, PersistError> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "DELETE")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

/// Extra facts stored next to the document (autosave uses them to describe the original file).
pub type Extra = Vec<(&'static str, String)>;

/// Writes `content` into the database at `path` (created if missing), incrementally.
pub fn save(path: &Path, content: &Content, name: &str, extra: &Extra, hashes: &HashCache) -> Result<(), PersistError> {
    use rayon::prelude::*;
    // hash every tile (cached) and collect the tile table rows
    let mut rows: Vec<(u64, i32, i32, Arc<Tile>)> = Vec::new();
    let mut mask_rows: Vec<(u64, i32, i32, Arc<MaskTile>)> = Vec::new();
    let mut nodes: Vec<(u64, Option<u64>, usize, &'static str, String)> = Vec::new();
    type MaskRows = Vec<(u64, i32, i32, Arc<MaskTile>)>;
    fn walk(list: &[Node], parent: Option<u64>, rows: &mut Vec<(u64, i32, i32, Arc<Tile>)>, mask_rows: &mut MaskRows, nodes: &mut Vec<(u64, Option<u64>, usize, &'static str, String)>) {
        for (i, n) in list.iter().enumerate() {
            let (kind, pass_through, expanded) = match &n.kind {
                NodeKind::Pixel { tiles } => {
                    for (&(tx, ty), t) in tiles.iter() {
                        rows.push((n.id, tx, ty, t.clone()));
                    }
                    ("pixel", false, false)
                }
                NodeKind::Group { pass_through, expanded, .. } => ("group", *pass_through, *expanded),
                NodeKind::Adjustment { .. } => ("adjustment", false, false),
                NodeKind::Text { tiles, .. } => {
                    for (&(tx, ty), t) in tiles.iter() {
                        rows.push((n.id, tx, ty, t.clone()));
                    }
                    ("text", false, false)
                }
            };
            let mask = n.mask.as_ref().map(|m| {
                for (&(tx, ty), t) in m.mask.tiles.iter() {
                    mask_rows.push((n.id, tx, ty, t.clone()));
                }
                MaskProps { enabled: m.enabled, density: m.density, feather: m.feather, outside: m.mask.outside }
            });
            let props = Props { name: n.name.clone(), visible: n.visible, opacity: n.opacity, fill: n.fill, blend: n.blend, locks: n.locks, clipped: n.clipped, pass_through, expanded, mask, adjustment: n.adjustment_settings().cloned(), text: n.text_data().cloned() };
            nodes.push((n.id, parent, i, kind, serde_json::to_string(&props).unwrap()));
            if let Some(c) = n.children() {
                walk(c, Some(n.id), rows, mask_rows, nodes);
            }
        }
    }
    walk(&content.root, None, &mut rows, &mut mask_rows, &mut nodes);
    let mask_hashed: Vec<Hash> = mask_rows.par_iter().map(|r| *blake3::hash(&r.3.0).as_bytes()).collect();
    let hashed: Vec<Hash> = rows.par_iter().map(|r| hashes.hash(&r.3)).collect();

    let mut conn = open_rw(path)?;
    let existing: HashSet<Hash> = {
        let mut st = conn.prepare("SELECT hash FROM blobs")?;
        let it = st.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
        it.filter_map(Result::ok).filter_map(|v| v.try_into().ok()).collect()
    };
    // compress only content the file doesn't have yet
    let mut seen = HashSet::new();
    let fresh: Vec<(Hash, Arc<Tile>)> =
        rows.iter().zip(&hashed).filter(|(_, h)| !existing.contains(*h) && seen.insert(**h)).map(|(r, h)| (*h, r.3.clone())).collect();
    let mut compressed: Vec<(Hash, Vec<u8>)> = fresh.par_iter().map(|(h, t)| (*h, lz4_flex::compress_prepend_size(&t.0))).collect();
    let fresh_masks: Vec<(Hash, Arc<MaskTile>)> =
        mask_rows.iter().zip(&mask_hashed).filter(|(_, h)| !existing.contains(*h) && seen.insert(**h)).map(|(r, h)| (*h, r.3.clone())).collect();
    compressed.extend(fresh_masks.par_iter().map(|(h, t)| (*h, lz4_flex::compress_prepend_size(&t.0))).collect::<Vec<_>>());

    let tx = conn.transaction()?;
    tx.execute("DELETE FROM nodes", [])?;
    tx.execute("DELETE FROM tiles", [])?;
    tx.execute("DELETE FROM masks", [])?;
    {
        let mut ins = tx.prepare("INSERT OR IGNORE INTO blobs (hash, data) VALUES (?1, ?2)")?;
        for (h, data) in &compressed {
            ins.execute(params![&h[..], data])?;
        }
        let mut ins = tx.prepare("INSERT INTO nodes (id, parent, idx, kind, props) VALUES (?1, ?2, ?3, ?4, ?5)")?;
        for (id, parent, idx, kind, props) in &nodes {
            ins.execute(params![*id as i64, parent.map(|p| p as i64), *idx as i64, kind, props])?;
        }
        let mut ins = tx.prepare("INSERT INTO tiles (node, tx, ty, hash) VALUES (?1, ?2, ?3, ?4)")?;
        for (r, h) in rows.iter().zip(&hashed) {
            ins.execute(params![r.0 as i64, r.1, r.2, &h[..]])?;
        }
        let mut ins = tx.prepare("INSERT INTO masks (node, tx, ty, hash) VALUES (?1, ?2, ?3, ?4)")?;
        for (r, h) in mask_rows.iter().zip(&mask_hashed) {
            ins.execute(params![r.0 as i64, r.1, r.2, &h[..]])?;
        }
    }
    tx.execute("DELETE FROM blobs WHERE hash NOT IN (SELECT hash FROM tiles UNION SELECT hash FROM masks)", [])?;
    tx.execute("DELETE FROM meta", [])?;
    {
        let mut ins = tx.prepare("INSERT INTO meta (key, value) VALUES (?1, ?2)")?;
        let base = [
            ("format", FORMAT.to_string()),
            ("app", format!("Lawha {}", env!("CARGO_PKG_VERSION"))),
            ("width", content.width.to_string()),
            ("height", content.height.to_string()),
            ("ppi", content.ppi.to_string()),
            ("active", content.active.to_string()),
            ("name", name.to_string()),
        ];
        for (k, v) in base.iter().map(|(k, v)| (*k, v.clone())).chain(extra.iter().cloned()) {
            ins.execute(params![k, v])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub struct Loaded {
    pub content: Content,
    pub name: String,
    pub meta: HashMap<String, String>,
}

pub fn read_meta(path: &Path) -> Result<HashMap<String, String>, PersistError> {
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut st = conn.prepare("SELECT key, value FROM meta").map_err(|_| PersistError::NotLawha)?;
    let meta: HashMap<String, String> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.filter_map(Result::ok).collect();
    match meta.get("format") {
        Some(f) if f == FORMAT => Ok(meta),
        Some(f) if f.starts_with("lawha/") => Err(PersistError::TooNew(f.clone())),
        _ => Err(PersistError::NotLawha),
    }
}

pub fn load(path: &Path) -> Result<Loaded, PersistError> {
    let meta = read_meta(path)?;
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let num = |k: &str| meta.get(k).and_then(|v| v.parse::<f64>().ok()).ok_or_else(|| PersistError::Damaged(format!("missing {k}")));
    let (width, height, ppi) = (num("width")? as u32, num("height")? as u32, num("ppi")?);

    // blobs → shared tiles
    let mut blobs: HashMap<Vec<u8>, Arc<Tile>> = HashMap::new();
    let mut mask_blobs: HashMap<Vec<u8>, Arc<MaskTile>> = HashMap::new();
    {
        let mut st = conn.prepare("SELECT hash, data FROM blobs")?;
        let rows: Vec<(Vec<u8>, Vec<u8>)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.filter_map(Result::ok).collect();
        use rayon::prelude::*;
        let decoded: Vec<(Vec<u8>, Result<Vec<u8>, String>)> =
            rows.into_par_iter().map(|(h, d)| (h, lz4_flex::decompress_size_prepended(&d).map_err(|e| e.to_string()))).collect();
        for (h, d) in decoded {
            let d = d.map_err(PersistError::Damaged)?;
            match d.len() {
                TILE_BYTES => {
                    blobs.insert(h, Arc::new(Tile(d.into_boxed_slice())));
                }
                MASK_TILE_BYTES => {
                    mask_blobs.insert(h, Arc::new(MaskTile(d.into_boxed_slice())));
                }
                _ => return Err(PersistError::Damaged("bad tile size".into())),
            }
        }
    }
    let mut tiles: HashMap<u64, TileMap> = HashMap::new();
    {
        let mut st = conn.prepare("SELECT node, tx, ty, hash FROM tiles")?;
        let it = st.query_map([], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i32>(1)?, r.get::<_, i32>(2)?, r.get::<_, Vec<u8>>(3)?)))?;
        for row in it {
            let (node, tx, ty, hash) = row?;
            let t = blobs.get(&hash).ok_or_else(|| PersistError::Damaged("missing tile data".into()))?;
            tiles.entry(node).or_default().insert((tx, ty), t.clone());
        }
    }
    let mut masks: HashMap<u64, MaskMap> = HashMap::new();
    if conn.prepare("SELECT 1 FROM masks LIMIT 1").is_ok() {
        let mut st = conn.prepare("SELECT node, tx, ty, hash FROM masks")?;
        let it = st.query_map([], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i32>(1)?, r.get::<_, i32>(2)?, r.get::<_, Vec<u8>>(3)?)))?;
        for row in it {
            let (node, tx, ty, hash) = row?;
            let t = mask_blobs.get(&hash).ok_or_else(|| PersistError::Damaged("missing mask data".into()))?;
            masks.entry(node).or_default().insert((tx, ty), t.clone());
        }
    }
    // nodes → tree
    let mut st = conn.prepare("SELECT id, parent, idx, kind, props FROM nodes ORDER BY idx")?;
    let rows: Vec<(u64, Option<u64>, i64, String, String)> =
        st.query_map([], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, Option<i64>>(1)?.map(|p| p as u64), r.get(2)?, r.get(3)?, r.get(4)?)))?.filter_map(Result::ok).collect();
    let mut by_parent: HashMap<Option<u64>, Vec<&(u64, Option<u64>, i64, String, String)>> = HashMap::new();
    let mut max_id = 0;
    for r in &rows {
        max_id = max_id.max(r.0);
        by_parent.entry(r.1).or_default().push(r);
    }
    type ByParent<'a> = HashMap<Option<u64>, Vec<&'a (u64, Option<u64>, i64, String, String)>>;
    fn build(parent: Option<u64>, by_parent: &ByParent, tiles: &mut HashMap<u64, TileMap>, masks: &mut HashMap<u64, MaskMap>, depth: usize) -> Result<Vec<Node>, PersistError> {
        if depth > 64 {
            return Err(PersistError::Damaged("layer tree too deep".into()));
        }
        let mut out = Vec::new();
        for (id, _, _, kind, props) in by_parent.get(&parent).map(|v| v.as_slice()).unwrap_or(&[]) {
            let p: Props = serde_json::from_str(props).map_err(|e| PersistError::Damaged(e.to_string()))?;
            let kind = match kind.as_str() {
                "group" => NodeKind::Group { children: build(Some(*id), by_parent, tiles, masks, depth + 1)?, pass_through: p.pass_through, expanded: p.expanded },
                "text" => {
                    let data = p.text.clone().ok_or_else(|| PersistError::Damaged("text layer without text".into()))?;
                    NodeKind::Text { data, tiles: Arc::new(tiles.remove(id).unwrap_or_default()) }
                }
                "adjustment" => {
                    let adj = p.adjustment.clone().ok_or_else(|| PersistError::Damaged("adjustment layer without settings".into()))?;
                    NodeKind::Adjustment { op: Arc::new(lawha_core::adjust::compile(&adj)), adj }
                }
                _ => NodeKind::Pixel { tiles: Arc::new(tiles.remove(id).unwrap_or_default()) },
            };
            let mask = p.mask.map(|m| LayerMask {
                mask: Mask { tiles: Arc::new(masks.remove(id).unwrap_or_default()), outside: m.outside },
                enabled: m.enabled,
                density: m.density,
                feather: m.feather,
            });
            out.push(Node { id: *id, name: p.name, visible: p.visible, opacity: p.opacity, fill: p.fill, blend: p.blend, locks: p.locks, clipped: p.clipped, mask, kind });
        }
        Ok(out)
    }
    let root = build(None, &by_parent, &mut tiles, &mut masks, 0)?;
    if root.is_empty() {
        return Err(PersistError::Damaged("no layers".into()));
    }
    reserve_ids_above(max_id);
    let active = meta.get("active").and_then(|v| v.parse().ok()).unwrap_or(root.last().unwrap().id);
    let name = meta.get("name").cloned().unwrap_or_default();
    Ok(Loaded { content: Content::new(width, height, ppi, active, root), name, meta })
}

/// True if the file at `path` is a Lawha database (checked by its meta table, not its extension).
pub fn is_lawha(path: &Path) -> bool {
    std::fs::File::open(path)
        .and_then(|mut f| {
            use std::io::Read;
            let mut head = [0u8; 16];
            f.read_exact(&mut head).map(|_| head)
        })
        .is_ok_and(|h| &h == b"SQLite format 3\0")
        && read_meta(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::composite::composite_lod0;
    use crate::doc::{filled_tiles, tiles_from_rgba};

    fn sample() -> Content {
        let mut a = Node::pixel("خلفية", filled_tiles(700, 500, [255, 255, 255, 255]));
        a.locks.position = true;
        let rgba: Vec<u8> = (0..700 * 500).flat_map(|i| [(i % 256) as u8, (i / 700 % 256) as u8, 77, (i % 200) as u8]).collect();
        let mut b = Node::pixel("طبقة 1", tiles_from_rgba(700, 500, &rgba));
        b.blend = BlendMode::Overlay;
        b.opacity = 0.6;
        b.fill = 0.8;
        let mut c = Node::pixel("clip", filled_tiles(700, 500, [200, 10, 10, 255]));
        c.clipped = true;
        let md = lawha_core::select::ellipse(700, 500, 100.0, 50.0, 600.0, 450.0, true);
        let mut lm = LayerMask::new(Mask::from_dense(&md, 0));
        lm.density = 0.8;
        lm.feather = 3.0;
        c.mask = Some(lm);
        let mut g = Node::group("مجموعة", vec![b, c]);
        g.opacity = 0.9;
        let active = g.id;
        let mut adj = Node::adjustment("منحنيات", lawha_core::adjust::Adjustment::Curves {
            channels: [vec![[0.0, 0.0], [90.0, 140.0], [255.0, 255.0]], vec![[0.0, 30.0], [255.0, 255.0]], vec![[0.0, 0.0], [255.0, 255.0]], vec![[0.0, 0.0], [255.0, 220.0]]],
        });
        adj.opacity = 0.7;
        let data = crate::doc::text::TextData {
            text: "مرحبا Hello ١٢٣".into(),
            style: lawha_core::text::TextStyle { size: 40.0, align: lawha_core::text::Align::Center, ..Default::default() },
            width: Some(300.0),
            height: Some(120.0),
            matrix: [1.0, 0.0, 50.0, 0.0, 1.0, 60.0],
        };
        let mut text = Node::pixel("نص", Default::default());
        text.kind = NodeKind::Text { tiles: Arc::new(crate::doc::text::render(&data, 700, 500)), data };
        Content::new(700, 500, 300.0, active, vec![a, g, adj, text])
    }

    fn same_tree(a: &[Node], b: &[Node]) {
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b) {
            assert_eq!((x.id, &x.name, x.visible, x.opacity, x.fill, x.blend, x.locks, x.clipped), (y.id, &y.name, y.visible, y.opacity, y.fill, y.blend, y.locks, y.clipped));
            assert_eq!(x.mask.is_some(), y.mask.is_some());
            if let (Some(a), Some(b)) = (&x.mask, &y.mask) {
                assert_eq!((a.enabled, a.density, a.feather, a.mask.outside), (b.enabled, b.density, b.feather, b.mask.outside));
                assert_eq!(a.mask.to_dense(700, 500), b.mask.to_dense(700, 500));
            }
            match (&x.kind, &y.kind) {
                (NodeKind::Pixel { tiles: t1 }, NodeKind::Pixel { tiles: t2 }) => {
                    assert_eq!(t1.len(), t2.len());
                    for (k, v) in t1.iter() {
                        assert_eq!(v.0, t2[k].0);
                    }
                }
                (NodeKind::Group { children: c1, pass_through: p1, .. }, NodeKind::Group { children: c2, pass_through: p2, .. }) => {
                    assert_eq!(p1, p2);
                    same_tree(c1, c2);
                }
                (NodeKind::Text { data: d1, tiles: t1 }, NodeKind::Text { data: d2, tiles: t2 }) => {
                    assert_eq!(d1, d2);
                    assert_eq!(t1.len(), t2.len());
                }
                (NodeKind::Adjustment { adj: a1, op: o1 }, NodeKind::Adjustment { adj: a2, op: o2 }) => {
                    assert_eq!(a1, a2);
                    assert_eq!(o1, o2);
                }
                _ => panic!("kind differs"),
            }
        }
    }

    #[test]
    fn save_and_load_round_trip_exactly() {
        let dir = tempdir();
        let path = dir.join("doc.lawha");
        let c = sample();
        save(&path, &c, "اختبار", &vec![], &HashCache::default()).unwrap();
        assert!(is_lawha(&path));
        let l = load(&path).unwrap();
        assert_eq!((l.content.width, l.content.height, l.content.ppi, l.content.active), (700, 500, 300.0, c.active));
        assert_eq!(l.name, "اختبار");
        same_tree(&c.root, &l.content.root);
        for (tx, ty) in [(0, 0), (1, 1), (2, 0)] {
            assert_eq!(composite_lod0(&c, tx, ty).map(|t| t.0), composite_lod0(&l.content, tx, ty).map(|t| t.0));
        }
    }

    #[test]
    fn identical_tiles_are_stored_once_and_resaves_are_incremental() {
        let dir = tempdir();
        let path = dir.join("doc.lawha");
        let hashes = HashCache::default();
        let w = Node::pixel("white", filled_tiles(2560, 2560, [255, 255, 255, 255]));
        let c = Content::new(2560, 2560, 72.0, w.id, vec![w]);
        save(&path, &c, "t", &vec![], &hashes).unwrap();
        let n: i64 = Connection::open(&path).unwrap().query_row("SELECT COUNT(*) FROM blobs", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "100 identical tiles → one blob");
        let size1 = std::fs::metadata(&path).unwrap().len();
        // re-saving an unchanged document writes no new blobs
        save(&path, &c, "t", &vec![], &hashes).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() <= size1 + 4096);
    }

    #[test]
    fn rejects_files_that_are_not_lawha() {
        let dir = tempdir();
        let p = dir.join("x.lawha");
        std::fs::write(&p, b"hello").unwrap();
        assert!(!is_lawha(&p));
        assert!(load(&p).is_err());
    }

    fn tempdir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("lawha-test-{}", crate::doc::next_id() * 7919 + std::process::id() as u64));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}
