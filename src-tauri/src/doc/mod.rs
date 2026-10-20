//! Documents live here, in the native process. The webview only holds the tiles it is displaying.

pub mod composite;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use lawha_core::tile::{Tile, TILE};
use lawha_core::BlendMode;
use parking_lot::RwLock;
use serde::Serialize;

pub type TileKey = (i32, i32);
pub type DocId = u64;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub struct Layer {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend: BlendMode,
    /// Sparse: a missing tile is fully transparent. Tiles are shared (`Arc`) and copied on write.
    pub tiles: HashMap<TileKey, Arc<Tile>>,
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Layer { id: next_id(), name: name.into(), visible: true, opacity: 1.0, blend: BlendMode::Normal, tiles: HashMap::new() }
    }

    /// A layer filled with one color over the whole document. Interior tiles share one allocation.
    pub fn filled(name: impl Into<String>, width: u32, height: u32, rgba: [u8; 4]) -> Self {
        let mut layer = Self::new(name);
        let full = Arc::new(Tile::filled(rgba));
        let (tw, th) = tiles_for(width, height);
        for ty in 0..th {
            for tx in 0..tw {
                let (x0, y0) = (tx as u32 * TILE as u32, ty as u32 * TILE as u32);
                let (w, h) = ((width - x0).min(TILE as u32), (height - y0).min(TILE as u32));
                let tile = if w == TILE as u32 && h == TILE as u32 {
                    full.clone()
                } else {
                    let mut t = Tile::transparent();
                    for y in 0..h as usize {
                        for x in 0..w as usize {
                            let i = (y * TILE + x) * 4;
                            t.0[i..i + 4].copy_from_slice(&rgba);
                        }
                    }
                    Arc::new(t)
                };
                layer.tiles.insert((tx, ty), tile);
            }
        }
        layer
    }

    /// A layer from a full RGBA8 image placed at the document origin. Empty tiles are skipped.
    pub fn from_rgba(name: impl Into<String>, width: u32, height: u32, rgba: &[u8]) -> Self {
        let mut layer = Self::new(name);
        let (tw, th) = tiles_for(width, height);
        let tiles: Vec<_> = (0..th)
            .flat_map(|ty| (0..tw).map(move |tx| (tx, ty)))
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|(tx, ty)| {
                let mut t = Tile::transparent();
                let (x0, y0) = (tx as usize * TILE, ty as usize * TILE);
                let w = (width as usize - x0).min(TILE);
                let h = (height as usize - y0).min(TILE);
                for y in 0..h {
                    let src = ((y0 + y) * width as usize + x0) * 4;
                    t.0[y * TILE * 4..y * TILE * 4 + w * 4].copy_from_slice(&rgba[src..src + w * 4]);
                }
                (!t.is_transparent()).then(|| ((tx, ty), Arc::new(t)))
            })
            .collect();
        layer.tiles.extend(tiles);
        layer
    }
}

pub fn tiles_for(width: u32, height: u32) -> (i32, i32) {
    (width.div_ceil(TILE as u32) as i32, height.div_ceil(TILE as u32) as i32)
}

pub struct Document {
    pub id: DocId,
    pub name: String,
    pub path: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
    pub ppi: f64,
    /// Bottom to top.
    pub layers: Vec<Layer>,
    pub active: u64,
    /// Bumped on every change; the webview uses it to drop stale tiles.
    pub version: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerInfo {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub blend: BlendMode,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocInfo {
    pub id: DocId,
    pub name: String,
    pub path: Option<String>,
    pub width: u32,
    pub height: u32,
    pub ppi: f64,
    /// Top to bottom, the order panels show.
    pub layers: Vec<LayerInfo>,
    pub active: u64,
    pub version: u64,
}

impl Document {
    pub fn info(&self) -> DocInfo {
        DocInfo {
            id: self.id,
            name: self.name.clone(),
            path: self.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            width: self.width,
            height: self.height,
            ppi: self.ppi,
            layers: self
                .layers
                .iter()
                .rev()
                .map(|l| LayerInfo { id: l.id, name: l.name.clone(), visible: l.visible, opacity: l.opacity, blend: l.blend })
                .collect(),
            active: self.active,
            version: self.version,
        }
    }
}

/// A document plus its composite cache.
pub struct DocHandle {
    pub doc: RwLock<Document>,
    pub composite: composite::Cache,
}

#[derive(Default)]
pub struct Engine {
    docs: RwLock<HashMap<DocId, Arc<DocHandle>>>,
}

impl Engine {
    pub fn insert(&self, doc: Document) -> Arc<DocHandle> {
        let id = doc.id;
        let h = Arc::new(DocHandle { doc: RwLock::new(doc), composite: composite::Cache::default() });
        self.docs.write().insert(id, h.clone());
        h
    }

    pub fn get(&self, id: DocId) -> Option<Arc<DocHandle>> {
        self.docs.read().get(&id).cloned()
    }

    pub fn remove(&self, id: DocId) {
        self.docs.write().remove(&id);
    }
}
