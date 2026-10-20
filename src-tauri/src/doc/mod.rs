//! Documents live here, in the native process. The webview only holds the tiles it is displaying.

pub mod adjustments;
pub mod composite;
pub mod filters;
pub mod history;
pub mod live;
pub mod model;
pub mod ops;
pub mod paint;
pub mod raster;
pub mod selection;
pub mod thumbs;
pub mod transform;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use lawha_core::tile::{Tile, TILE};
use lawha_core::BlendMode;
use parking_lot::RwLock;
use serde::Serialize;

pub use history::{History, Label};
pub use model::{Content, Locks, Node, NodeKind, TileMap};

pub type DocId = u64;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Ids loaded from a file must never collide with ids handed out later.
pub fn reserve_ids_above(id: u64) {
    NEXT_ID.fetch_max(id + 1, Ordering::Relaxed);
}

/// Tiles of a layer filled with one color over the whole canvas. Interior tiles share one allocation.
pub fn filled_tiles(width: u32, height: u32, rgba: [u8; 4]) -> TileMap {
    let mut map = TileMap::new();
    let full = Arc::new(Tile::filled(rgba));
    let (tw, th) = (width.div_ceil(TILE as u32) as i32, height.div_ceil(TILE as u32) as i32);
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
            map.insert((tx, ty), tile);
        }
    }
    map
}

/// Tiles from a full RGBA8 image placed at the canvas origin. Empty tiles are skipped.
pub fn tiles_from_rgba(width: u32, height: u32, rgba: &[u8]) -> TileMap {
    use rayon::prelude::*;
    let (tw, th) = (width.div_ceil(TILE as u32) as i32, height.div_ceil(TILE as u32) as i32);
    (0..th)
        .flat_map(|ty| (0..tw).map(move |tx| (tx, ty)))
        .collect::<Vec<_>>()
        .into_par_iter()
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
        .collect()
}

pub struct Document {
    pub id: DocId,
    pub name: String,
    pub path: Option<PathBuf>,
    pub content: Content,
    pub history: History,
    /// Bumped on every change; the webview uses it to drop stale tiles.
    pub version: u64,
    /// History index at the last save, if any.
    pub saved_at: Option<usize>,
    /// Bumped when content changes; autosave compares it.
    pub change_counter: u64,
    /// The selection before the last deselect, for Reselect.
    pub last_selection: Option<lawha_core::mask::Mask>,
    /// A refine-edge preview shown instead of the selection (not in history).
    pub preview: Option<lawha_core::mask::Mask>,
}

impl Document {
    pub fn new(name: String, path: Option<PathBuf>, content: Content, label: Label) -> Self {
        Document { id: next_id(), name, path, history: History::new(content.clone(), label), content, version: 1, saved_at: None, change_counter: 0, last_selection: None, preview: None }
    }

    pub fn touch(&mut self) {
        self.version += 1;
        self.change_counter += 1;
    }

    /// Applies a change and records it as one history step.
    pub fn commit(&mut self, label: Label) {
        self.history.push(label, self.content.clone());
        self.touch();
    }

    pub fn undo(&mut self) -> bool {
        if !self.history.can_undo() {
            return false;
        }
        let i = self.history.current - 1;
        self.jump(i)
    }

    pub fn redo(&mut self) -> bool {
        if !self.history.can_redo() {
            return false;
        }
        let i = self.history.current + 1;
        self.jump(i)
    }

    pub fn jump(&mut self, index: usize) -> bool {
        match self.history.jump(index) {
            Some(c) => {
                self.content = (*c).clone();
                self.touch();
                true
            }
            None => false,
        }
    }

    pub fn dirty(&self) -> bool {
        self.saved_at != Some(self.history.current)
    }

    pub fn info(&self) -> DocInfo {
        fn layer(n: &Node) -> LayerInfo {
            let (kind, pass_through, expanded, children, thumb) = match &n.kind {
                NodeKind::Pixel { tiles } => ("pixel", false, false, None, Arc::as_ptr(tiles) as usize as u64),
                NodeKind::Group { children, pass_through, expanded } => {
                    ("group", *pass_through, *expanded, Some(children.iter().rev().map(layer).collect()), 0)
                }
                NodeKind::Adjustment { .. } => ("adjustment", false, false, None, 0),
            };
            LayerInfo {
                mask: n.mask.as_ref().map(|m| MaskInfo {
                    enabled: m.enabled,
                    density: m.density,
                    feather: m.feather,
                    thumb: Arc::as_ptr(&m.mask.tiles) as usize as u64 ^ m.mask.outside as u64,
                }),
                id: n.id,
                name: n.name.clone(),
                kind,
                visible: n.visible,
                opacity: n.opacity,
                fill: n.fill,
                blend: n.blend,
                pass_through,
                expanded,
                locks: n.locks,
                clipped: n.clipped,
                thumb,
                adjustment: n.adjustment_settings().cloned(),
                children,
            }
        }
        DocInfo {
            id: self.id,
            name: self.name.clone(),
            path: self.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            width: self.content.width,
            height: self.content.height,
            ppi: self.content.ppi,
            layers: self.content.root.iter().rev().map(layer).collect(),
            active: self.content.active,
            version: self.version,
            dirty: self.dirty(),
            has_selection: self.content.selection.is_some() || self.preview.is_some(),
            has_last_selection: self.last_selection.is_some(),
            refining: self.preview.is_some(),
            selection_key: match self.preview.as_ref().or(self.content.selection.as_ref()) {
                Some(m) => (Arc::as_ptr(&m.tiles) as usize as u64) ^ ((m.outside as u64) << 1) | 1,
                None => 0,
            },
            history: HistoryInfo {
                labels: self.history.states.iter().map(|s| s.label.clone()).collect(),
                current: self.history.current,
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerInfo {
    pub id: u64,
    pub name: String,
    pub kind: &'static str,
    pub visible: bool,
    pub opacity: f32,
    pub fill: f32,
    pub blend: BlendMode,
    pub pass_through: bool,
    pub expanded: bool,
    pub locks: Locks,
    pub clipped: bool,
    pub mask: Option<MaskInfo>,
    /// Changes whenever the layer's pixels change (thumbnail cache key).
    pub thumb: u64,
    /// The settings of an adjustment layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adjustment: Option<lawha_core::adjust::Adjustment>,
    /// Top to bottom.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<LayerInfo>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskInfo {
    pub enabled: bool,
    pub density: f32,
    pub feather: f32,
    /// Changes whenever the mask pixels change (thumbnail cache key).
    pub thumb: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryInfo {
    pub labels: Vec<Label>,
    pub current: usize,
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
    pub dirty: bool,
    pub history: HistoryInfo,
    /// True when something is selected (or a refine preview is showing).
    pub has_selection: bool,
    pub has_last_selection: bool,
    pub refining: bool,
    /// Changes only when the selection or the refine preview changes; 0 when nothing is selected.
    pub selection_key: u64,
}

/// A document plus its caches.
pub struct DocHandle {
    pub doc: RwLock<Document>,
    pub composite: composite::Cache,
    pub selection: composite::SelCache,
    pub thumbs: thumbs::Cache,
}

#[derive(Default)]
pub struct Engine {
    docs: RwLock<HashMap<DocId, Arc<DocHandle>>>,
    live: RwLock<HashMap<u64, Arc<live::Session>>>,
    pub strokes: paint::Strokes,
}

impl Engine {
    pub fn insert(&self, doc: Document) -> Arc<DocHandle> {
        let id = doc.id;
        let h = Arc::new(DocHandle { doc: RwLock::new(doc), composite: composite::Cache::default(), selection: composite::SelCache::default(), thumbs: thumbs::Cache::default() });
        self.docs.write().insert(id, h.clone());
        h
    }

    pub fn get(&self, id: DocId) -> Option<Arc<DocHandle>> {
        self.docs.read().get(&id).cloned()
    }

    pub fn remove(&self, id: DocId) {
        self.docs.write().remove(&id);
        self.live.write().retain(|_, s| s.doc != id);
    }

    /// Starts a live session for a layer; None when the GPU can't reproduce the composite exactly.
    pub fn live_begin(&self, doc: DocId, layer: u64) -> Option<u64> {
        let h = self.get(doc)?;
        let content = h.doc.read().content.clone();
        // reuse the document's session when only this layer changed since it was made: its below
        // and above parts (and the GPU's copies of them) stay valid
        let fp = live::Session::matches(&content, layer)?;
        let existing = self.live.read().values().find(|s| s.doc == doc && s.layer == layer && s.fingerprint == fp && s.reusable()).cloned();
        if let Some((id, s)) = existing.map(|s| (self.live.read().iter().find(|(_, x)| Arc::ptr_eq(x, &s)).map(|(id, _)| *id), s)) {
            if let (Some(id), Some(node)) = (id, content.find(layer)) {
                s.reset_layer(node);
                return Some(id);
            }
        }
        let id = next_id();
        let s = live::Session::begin(doc, &content, layer)?;
        let mut live = self.live.write();
        live.retain(|_, x| x.doc != doc); // one session per document
        live.insert(id, Arc::new(s));
        Some(id)
    }

    /// A session for previewing a transform of one layer (see `Session::begin_transform`).
    pub fn live_begin_transform(&self, doc: DocId, layer: u64) -> Option<u64> {
        let h = self.get(doc)?;
        let content = h.doc.read().content.clone();
        let s = live::Session::begin_transform(doc, &content, layer)?;
        let id = next_id();
        let mut live = self.live.write();
        live.retain(|_, x| x.doc != doc);
        live.insert(id, Arc::new(s));
        Some(id)
    }

    /// A whole-document session for previews the split can't serve (see `Session::begin_full`).
    pub fn live_begin_full(&self, doc: DocId, layer: u64) -> Option<u64> {
        let h = self.get(doc)?;
        let content = h.doc.read().content.clone();
        let s = live::Session::begin_full(doc, &content, layer)?;
        let id = next_id();
        let mut live = self.live.write();
        live.retain(|_, x| x.doc != doc);
        live.insert(id, Arc::new(s));
        Some(id)
    }

    pub fn live(&self, session: u64) -> Option<Arc<live::Session>> {
        self.live.read().get(&session).cloned()
    }

    pub fn live_end(&self, session: u64) {
        self.live.write().remove(&session);
    }

    pub fn all(&self) -> Vec<Arc<DocHandle>> {
        self.docs.read().values().cloned().collect()
    }
}
