//! The layer tree. Cloning a `Content` is cheap: pixel data sits behind `Arc`s and is shared until a
//! layer is edited, which is what makes every history state a full, independent snapshot.

use std::collections::HashMap;
use std::sync::Arc;

use lawha_core::adjust::{self, Adjustment, Op};
use lawha_core::mask::Mask;
use lawha_core::tile::{Tile, TILE};
use lawha_core::BlendMode;
use serde::{Deserialize, Serialize};

pub type TileKey = (i32, i32);
pub type TileMap = HashMap<TileKey, Arc<Tile>>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Locks {
    pub transparent: bool,
    pub pixels: bool,
    pub position: bool,
}

#[derive(Clone)]
pub enum NodeKind {
    Pixel { tiles: Arc<TileMap> },
    Group { children: Vec<Node>, pass_through: bool, expanded: bool },
    /// Changes the colors of everything below it (in its group, or its clip base when clipped).
    /// `op` is `adj` compiled; keep them in step with `Node::set_adjustment`.
    Adjustment { adj: Adjustment, op: Arc<Op> },
    /// Editable text, with its last drawing (`tiles`) for compositing.
    Text { data: super::text::TextData, tiles: Arc<TileMap> },
}

/// A layer mask: white shows, black hides.
#[derive(Clone)]
pub struct LayerMask {
    pub mask: Mask,
    pub enabled: bool,
    /// 0–1: 1 = the mask's full effect, 0 = no effect.
    pub density: f32,
    /// Render-time blur of the mask edge, px.
    pub feather: f32,
}

impl LayerMask {
    pub fn new(mask: Mask) -> Self {
        LayerMask { mask, enabled: true, density: 1.0, feather: 0.0 }
    }
}

#[derive(Clone)]
pub struct Node {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    /// Fades everything the layer draws, including its styles.
    pub opacity: f32,
    /// Fades only the layer's own pixels (pixel layers).
    pub fill: f32,
    pub blend: BlendMode,
    pub locks: Locks,
    /// Clipped to the nearest unclipped sibling below.
    pub clipped: bool,
    pub mask: Option<LayerMask>,
    pub kind: NodeKind,
}

impl Node {
    pub fn pixel(name: impl Into<String>, tiles: TileMap) -> Node {
        Node {
            id: super::next_id(),
            name: name.into(),
            visible: true,
            opacity: 1.0,
            fill: 1.0,
            blend: BlendMode::Normal,
            locks: Locks::default(),
            clipped: false,
            mask: None,
            kind: NodeKind::Pixel { tiles: Arc::new(tiles) },
        }
    }

    pub fn group(name: impl Into<String>, children: Vec<Node>) -> Node {
        Node {
            id: super::next_id(),
            name: name.into(),
            visible: true,
            opacity: 1.0,
            fill: 1.0,
            blend: BlendMode::Normal,
            locks: Locks::default(),
            clipped: false,
            mask: None,
            kind: NodeKind::Group { children, pass_through: true, expanded: true },
        }
    }

    /// An adjustment layer, with the implicit white mask other editors give it.
    pub fn adjustment(name: impl Into<String>, adj: Adjustment) -> Node {
        let mut n = Node::pixel(name, TileMap::new());
        n.mask = Some(LayerMask::new(Mask::uniform(255)));
        n.kind = NodeKind::Adjustment { op: Arc::new(adjust::compile(&adj)), adj };
        n
    }

    pub fn adjustment_settings(&self) -> Option<&Adjustment> {
        match &self.kind {
            NodeKind::Adjustment { adj, .. } => Some(adj),
            _ => None,
        }
    }

    pub fn set_adjustment(&mut self, a: Adjustment) {
        self.kind = NodeKind::Adjustment { op: Arc::new(adjust::compile(&a)), adj: a };
    }

    pub fn text_data(&self) -> Option<&super::text::TextData> {
        match &self.kind {
            NodeKind::Text { data, .. } => Some(data),
            _ => None,
        }
    }

    pub fn is_adjustment(&self) -> bool {
        matches!(self.kind, NodeKind::Adjustment { .. })
    }

    pub fn is_group(&self) -> bool {
        matches!(self.kind, NodeKind::Group { .. })
    }

    pub fn children(&self) -> Option<&Vec<Node>> {
        match &self.kind {
            NodeKind::Group { children, .. } => Some(children),
            _ => None,
        }
    }

    pub fn children_mut(&mut self) -> Option<&mut Vec<Node>> {
        match &mut self.kind {
            NodeKind::Group { children, .. } => Some(children),
            _ => None,
        }
    }

    pub fn tiles(&self) -> Option<&Arc<TileMap>> {
        match &self.kind {
            NodeKind::Pixel { tiles } | NodeKind::Text { tiles, .. } => Some(tiles),
            _ => None,
        }
    }

    /// Gives every node in this subtree a fresh id (used by duplicate).
    pub fn reidentify(&mut self) {
        self.id = super::next_id();
        if let Some(c) = self.children_mut() {
            for n in c {
                n.reidentify();
            }
        }
    }
}

/// Everything history snapshots: the canvas and the layer tree.
#[derive(Clone)]
pub struct Content {
    pub width: u32,
    pub height: u32,
    pub ppi: f64,
    /// Bottom to top.
    pub root: Vec<Node>,
    pub active: u64,
    /// The current selection; None when nothing is selected.
    pub selection: Option<Mask>,
}

/// Position of a node: the id of its parent group (None = root) and its index there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub parent: Option<u64>,
    pub index: usize,
}

impl Content {
    pub fn new(width: u32, height: u32, ppi: f64, active: u64, root: Vec<Node>) -> Content {
        Content { width, height, ppi, root, active, selection: None }
    }

    pub fn find(&self, id: u64) -> Option<&Node> {
        fn walk(nodes: &[Node], id: u64) -> Option<&Node> {
            for n in nodes {
                if n.id == id {
                    return Some(n);
                }
                if let Some(c) = n.children() {
                    if let Some(f) = walk(c, id) {
                        return Some(f);
                    }
                }
            }
            None
        }
        walk(&self.root, id)
    }

    pub fn find_mut(&mut self, id: u64) -> Option<&mut Node> {
        fn walk(nodes: &mut [Node], id: u64) -> Option<&mut Node> {
            for n in nodes {
                if n.id == id {
                    return Some(n);
                }
                if let Some(c) = n.children_mut() {
                    if let Some(f) = walk(c, id) {
                        return Some(f);
                    }
                }
            }
            None
        }
        walk(&mut self.root, id)
    }

    pub fn slot_of(&self, id: u64) -> Option<Slot> {
        fn walk(nodes: &[Node], parent: Option<u64>, id: u64) -> Option<Slot> {
            for (i, n) in nodes.iter().enumerate() {
                if n.id == id {
                    return Some(Slot { parent, index: i });
                }
                if let Some(c) = n.children() {
                    if let Some(s) = walk(c, Some(n.id), id) {
                        return Some(s);
                    }
                }
            }
            None
        }
        walk(&self.root, None, id)
    }

    pub fn siblings_mut(&mut self, parent: Option<u64>) -> Option<&mut Vec<Node>> {
        match parent {
            None => Some(&mut self.root),
            Some(p) => self.find_mut(p)?.children_mut(),
        }
    }

    pub fn siblings(&self, parent: Option<u64>) -> Option<&Vec<Node>> {
        match parent {
            None => Some(&self.root),
            Some(p) => self.find(p)?.children(),
        }
    }

    pub fn remove(&mut self, id: u64) -> Option<(Node, Slot)> {
        let slot = self.slot_of(id)?;
        let sib = self.siblings_mut(slot.parent)?;
        Some((sib.remove(slot.index), slot))
    }

    pub fn insert(&mut self, node: Node, slot: Slot) -> bool {
        match self.siblings_mut(slot.parent) {
            Some(sib) => {
                let i = slot.index.min(sib.len());
                sib.insert(i, node);
                true
            }
            None => false,
        }
    }

    /// True if `ancestor` contains `id` somewhere below it.
    pub fn is_descendant(&self, id: u64, ancestor: u64) -> bool {
        self.find(ancestor).and_then(|a| a.children()).is_some_and(|c| {
            fn walk(nodes: &[Node], id: u64) -> bool {
                nodes.iter().any(|n| n.id == id || n.children().is_some_and(|c| walk(c, id)))
            }
            walk(c, id)
        })
    }

    /// All ids, depth first, bottom to top.
    pub fn ids(&self) -> Vec<u64> {
        fn walk(nodes: &[Node], out: &mut Vec<u64>) {
            for n in nodes {
                out.push(n.id);
                if let Some(c) = n.children() {
                    walk(c, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &mut out);
        out
    }

    pub fn tiles_wide(&self) -> (i32, i32) {
        (self.width.div_ceil(TILE as u32) as i32, self.height.div_ceil(TILE as u32) as i32)
    }
}
