//! Layer operations. Each one edits `doc.content` and records one history step.

use std::collections::HashSet;
use std::sync::Arc;

use mosamem_core::tile::{Accum, Tile};
use mosamem_core::BlendMode;
use serde::Deserialize;

use super::composite::{composite_nodes, composite_stack};
use super::model::{Locks, Node, NodeKind, Slot, TileMap};
use super::{Document, Label};

pub type OpResult = Result<(), String>;

pub(crate) fn active_slot(doc: &Document) -> Slot {
    doc.content.slot_of(doc.content.active).unwrap_or(Slot { parent: None, index: doc.content.root.len().saturating_sub(1) })
}

/// A new empty pixel layer directly above the active layer.
pub fn new_layer(doc: &mut Document, name: String) -> OpResult {
    let slot = active_slot(doc);
    let node = Node::pixel(name.clone(), TileMap::new());
    let id = node.id;
    doc.content.insert(node, Slot { parent: slot.parent, index: slot.index + 1 });
    doc.content.active = id;
    doc.commit(Label::with("history.newLayer", name));
    Ok(())
}

/// Ids sorted bottom to top in tree order, with descendants of other listed ids removed.
fn normalize(doc: &Document, ids: &[u64]) -> Vec<u64> {
    let want: HashSet<u64> = ids.iter().copied().collect();
    let order = doc.content.ids();
    order
        .into_iter()
        .filter(|id| want.contains(id))
        .filter(|id| !ids.iter().any(|a| a != id && doc.content.is_descendant(*id, *a)))
        .collect()
}

/// Wraps the given layers in a new group (or creates an empty group above the active layer).
pub fn new_group(doc: &mut Document, name: String, ids: &[u64]) -> OpResult {
    let ids = normalize(doc, ids);
    if ids.is_empty() {
        let slot = active_slot(doc);
        let g = Node::group(name.clone(), Vec::new());
        let gid = g.id;
        doc.content.insert(g, Slot { parent: slot.parent, index: slot.index + 1 });
        doc.content.active = gid;
        doc.commit(Label::with("history.newGroup", name));
        return Ok(());
    }
    // the group goes where the topmost selected layer was
    let top = *ids.last().unwrap();
    let target = doc.content.slot_of(top).ok_or("layer not found")?;
    let mut children = Vec::new();
    let mut insert_at = target;
    for id in &ids {
        let (n, slot) = doc.content.remove(*id).ok_or("layer not found")?;
        if slot.parent == insert_at.parent && slot.index < insert_at.index {
            insert_at.index -= 1;
        }
        children.push(n);
    }
    let g = Node::group(name.clone(), children);
    let gid = g.id;
    doc.content.insert(g, insert_at);
    doc.content.active = gid;
    doc.commit(Label::with("history.group", name));
    Ok(())
}

pub fn ungroup(doc: &mut Document, id: u64) -> OpResult {
    let (node, slot) = doc.content.remove(id).ok_or("layer not found")?;
    let NodeKind::Group { children, .. } = node.kind else {
        doc.content.insert(node, slot);
        return Err("not a group".into());
    };
    let top = children.last().map(|n| n.id);
    for (k, child) in children.into_iter().enumerate() {
        doc.content.insert(child, Slot { parent: slot.parent, index: slot.index + k });
    }
    if let Some(t) = top {
        doc.content.active = t;
    }
    doc.commit(Label::new("history.ungroup"));
    Ok(())
}

pub fn duplicate(doc: &mut Document, ids: &[u64], suffix: &str) -> OpResult {
    let ids = normalize(doc, ids);
    let mut last = None;
    for id in ids.iter().rev() {
        let slot = doc.content.slot_of(*id).ok_or("layer not found")?;
        let mut copy = doc.content.find(*id).unwrap().clone();
        copy.reidentify();
        copy.name = format!("{}{}", copy.name, suffix);
        last.get_or_insert(copy.id);
        doc.content.insert(copy, Slot { parent: slot.parent, index: slot.index + 1 });
    }
    if let Some(a) = last {
        doc.content.active = a;
    }
    doc.commit(Label::new("history.duplicate"));
    Ok(())
}

pub fn delete(doc: &mut Document, ids: &[u64]) -> OpResult {
    let ids = normalize(doc, ids);
    let top_level_left = doc.content.root.iter().filter(|n| !ids.contains(&n.id)).count();
    if top_level_left == 0 {
        return Err("a document needs at least one layer".into());
    }
    let mut fallback = None;
    for id in &ids {
        if let Some((_, slot)) = doc.content.remove(*id) {
            fallback = Some(slot);
        }
    }
    if doc.content.find(doc.content.active).is_none() {
        let slot = fallback.unwrap_or(Slot { parent: None, index: 0 });
        let pick = doc
            .content
            .siblings(slot.parent)
            .filter(|s| !s.is_empty())
            .map(|s| s[slot.index.saturating_sub(1).min(s.len() - 1)].id)
            .or(slot.parent)
            .unwrap_or_else(|| doc.content.root.last().unwrap().id);
        doc.content.active = pick;
    }
    doc.commit(Label::new("history.delete"));
    Ok(())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LayerProps {
    pub name: Option<String>,
    pub visible: Option<bool>,
    pub opacity: Option<f32>,
    pub fill: Option<f32>,
    pub blend: Option<BlendMode>,
    pub pass_through: Option<bool>,
    pub locks: Option<Locks>,
    pub clipped: Option<bool>,
    pub expanded: Option<bool>,
}

/// Sets layer properties. `transient` changes (slider drags) update the document without a history
/// step; the final value is committed with `transient = false`.
pub fn set_props(doc: &mut Document, id: u64, p: LayerProps, transient: bool) -> OpResult {
    let node = doc.content.find_mut(id).ok_or("layer not found")?;
    let mut label = None;
    if let Some(v) = p.name {
        label = Some(Label::with("history.rename", v.clone()));
        node.name = v;
    }
    if let Some(v) = p.visible {
        label = Some(Label::new(if v { "history.show" } else { "history.hide" }));
        node.visible = v;
    }
    if let Some(v) = p.opacity {
        label = Some(Label::new("history.opacity"));
        node.opacity = v.clamp(0.0, 1.0);
    }
    if let Some(v) = p.fill {
        label = Some(Label::new("history.fill"));
        node.fill = v.clamp(0.0, 1.0);
    }
    if let Some(v) = p.blend {
        label = Some(Label::new("history.blend"));
        node.blend = v;
        if let NodeKind::Group { pass_through, .. } = &mut node.kind {
            *pass_through = false;
        }
    }
    if let Some(v) = p.pass_through {
        if let NodeKind::Group { pass_through, .. } = &mut node.kind {
            label = Some(Label::new("history.blend"));
            *pass_through = v;
        }
    }
    if let Some(v) = p.locks {
        label = Some(Label::new("history.locks"));
        node.locks = v;
    }
    if let Some(v) = p.clipped {
        label = Some(Label::new(if v { "history.clip" } else { "history.unclip" }));
        node.clipped = v;
    }
    if let Some(v) = p.expanded {
        if let NodeKind::Group { expanded, .. } = &mut node.kind {
            *expanded = v;
        }
    }
    match label {
        Some(l) if !transient => doc.commit(l),
        _ => doc.touch(),
    }
    Ok(())
}

/// Shows only this layer (and its ancestors), or shows everything again if it already was alone.
pub fn solo(doc: &mut Document, id: u64) -> OpResult {
    let ids = doc.content.ids();
    let keep: HashSet<u64> = ids.iter().copied().filter(|x| *x == id || doc.content.is_descendant(id, *x) || doc.content.is_descendant(*x, id)).collect();
    let alone = ids.iter().all(|x| keep.contains(x) || !doc.content.find(*x).unwrap().visible);
    for x in ids {
        let n = doc.content.find_mut(x).unwrap();
        n.visible = alone || keep.contains(&x);
    }
    doc.commit(Label::new("history.visibility"));
    Ok(())
}

/// Moves layers to `parent` at `index` (index counted bottom to top, before removal).
pub fn move_layers(doc: &mut Document, ids: &[u64], parent: Option<u64>, index: usize) -> OpResult {
    let ids = normalize(doc, ids);
    if let Some(p) = parent {
        if ids.iter().any(|id| *id == p || doc.content.is_descendant(p, *id)) {
            return Err("a group can't go inside itself".into());
        }
        if !doc.content.find(p).is_some_and(Node::is_group) {
            return Err("target is not a group".into());
        }
    }
    let mut at = index;
    let mut nodes = Vec::new();
    for id in &ids {
        let (n, slot) = doc.content.remove(*id).ok_or("layer not found")?;
        if slot.parent == parent && slot.index < at {
            at -= 1;
        }
        nodes.push(n);
    }
    for (k, n) in nodes.into_iter().enumerate() {
        doc.content.insert(n, Slot { parent, index: at + k });
    }
    doc.commit(Label::new("history.move"));
    Ok(())
}

/// Pixels of a node as rendered on its own (isolated), with fill baked in.
fn rasterize(node: &Node) -> TileMap {
    let keys = tile_keys(node);
    use rayon::prelude::*;
    let mut probe = node.clone();
    probe.opacity = 1.0;
    probe.blend = BlendMode::Normal;
    probe.visible = true;
    probe.clipped = false;
    if let NodeKind::Group { pass_through, .. } = &mut probe.kind {
        *pass_through = false;
    }
    let one = [probe];
    keys.into_par_iter().filter_map(|(tx, ty)| composite_nodes(&one, tx, ty).map(|t| ((tx, ty), Arc::new(t)))).collect()
}

fn tile_keys(node: &Node) -> Vec<(i32, i32)> {
    let mut keys = HashSet::new();
    fn walk(n: &Node, keys: &mut HashSet<(i32, i32)>) {
        match &n.kind {
            NodeKind::Pixel { tiles } | NodeKind::Text { tiles, .. } => keys.extend(tiles.keys().copied()),
            NodeKind::Group { children, .. } => children.iter().for_each(|c| walk(c, keys)),
            NodeKind::Adjustment { .. } => {}
        }
    }
    walk(node, &mut keys);
    keys.into_iter().collect()
}

/// Merges the layer into the one below it. The result keeps the lower layer's name, mode and opacity.
pub fn merge_down(doc: &mut Document, id: u64) -> OpResult {
    let slot = doc.content.slot_of(id).ok_or("layer not found")?;
    if slot.index == 0 {
        return Err("no layer below".into());
    }
    let sib = doc.content.siblings(slot.parent).unwrap();
    let (below, top) = (&sib[slot.index - 1], &sib[slot.index]);
    if below.is_group() {
        return Err("can't merge into a group".into());
    }
    if below.is_adjustment() {
        return Err("can't merge into an adjustment layer".into());
    }
    let below_tiles = rasterize(below);
    let mut keys: HashSet<(i32, i32)> = below_tiles.keys().copied().collect();
    keys.extend(tile_keys(top));
    let top_alone = {
        let mut t = top.clone();
        t.clipped = false;
        vec![t]
    };
    use rayon::prelude::*;
    let merged: TileMap = keys
        .into_par_iter()
        .filter_map(|(tx, ty)| {
            let origin = (tx as i64 * 256, ty as i64 * 256);
            let mut acc = match below_tiles.get(&(tx, ty)) {
                Some(t) => Accum::from_tile(t, 1.0),
                None => Accum::new(),
            };
            composite_stack(&top_alone, &mut acc, 0, (tx, ty), origin);
            if acc.is_empty() {
                return None;
            }
            let mut out = Tile::transparent();
            acc.to_tile(&mut out);
            Some(((tx, ty), Arc::new(out)))
        })
        .collect();
    let below_id = below.id;
    doc.content.remove(id);
    let b = doc.content.find_mut(below_id).unwrap();
    b.kind = NodeKind::Pixel { tiles: Arc::new(merged) };
    b.fill = 1.0;
    doc.content.active = below_id;
    doc.commit(Label::new("history.mergeDown"));
    Ok(())
}

/// Merges all visible top-level layers into one layer; hidden layers stay as they are.
pub fn merge_visible(doc: &mut Document, name: String) -> OpResult {
    let visible: Vec<Node> = doc.content.root.iter().filter(|n| n.visible).cloned().collect();
    if visible.len() < 2 {
        return Err("nothing to merge".into());
    }
    let tiles = flatten_nodes(&visible, doc.content.tiles_wide());
    let first = doc.content.root.iter().position(|n| n.visible).unwrap();
    doc.content.root.retain(|n| !n.visible);
    let layer = Node::pixel(name, tiles);
    doc.content.active = layer.id;
    doc.content.root.insert(first.min(doc.content.root.len()), layer);
    doc.commit(Label::new("history.mergeVisible"));
    Ok(())
}

/// Flattens everything into one layer over white, like a printed page.
pub fn flatten(doc: &mut Document, name: String) -> OpResult {
    let (w, h) = (doc.content.width, doc.content.height);
    let mut nodes = vec![Node::pixel("white", super::filled_tiles(w, h, [255, 255, 255, 255]))];
    nodes.extend(doc.content.root.iter().filter(|n| n.visible).cloned());
    let tiles = flatten_nodes(&nodes, doc.content.tiles_wide());
    let layer = Node::pixel(name, tiles);
    doc.content.active = layer.id;
    doc.content.root = vec![layer];
    doc.commit(Label::new("history.flatten"));
    Ok(())
}

fn flatten_nodes(nodes: &[Node], (tw, th): (i32, i32)) -> TileMap {
    use rayon::prelude::*;
    (0..th)
        .flat_map(|ty| (0..tw).map(move |tx| (tx, ty)))
        .collect::<Vec<_>>()
        .into_par_iter()
        .filter_map(|(tx, ty)| composite_nodes(nodes, tx, ty).map(|t| ((tx, ty), Arc::new(t))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::composite::composite_lod0;
    use crate::doc::{filled_tiles, Content};

    fn doc_with(names: &[&str]) -> Document {
        let root: Vec<Node> = names.iter().map(|n| Node::pixel(*n, filled_tiles(256, 256, [10, 20, 30, 255]))).collect();
        let active = root.last().unwrap().id;
        Document::new("t".into(), None, Content::new(256, 256, 72.0, active, root), Label::new("history.new"))
    }

    fn names(nodes: &[Node]) -> Vec<String> {
        nodes.iter().map(|n| n.name.clone()).collect()
    }

    #[test]
    fn new_layer_goes_above_the_active_layer_and_undo_removes_it() {
        let mut d = doc_with(&["a", "b"]);
        d.content.active = d.content.root[0].id;
        new_layer(&mut d, "n".into()).unwrap();
        assert_eq!(names(&d.content.root), ["a", "n", "b"]);
        assert!(d.undo());
        assert_eq!(names(&d.content.root), ["a", "b"]);
        assert!(d.redo());
        assert_eq!(names(&d.content.root), ["a", "n", "b"]);
    }

    #[test]
    fn grouping_and_ungrouping_keep_order() {
        let mut d = doc_with(&["a", "b", "c", "d"]);
        let ids: Vec<u64> = d.content.root.iter().map(|n| n.id).collect();
        new_group(&mut d, "g".into(), &[ids[2], ids[1]]).unwrap();
        assert_eq!(names(&d.content.root), ["a", "g", "d"]);
        assert_eq!(names(d.content.root[1].children().unwrap()), ["b", "c"]);
        let g = d.content.root[1].id;
        ungroup(&mut d, g).unwrap();
        assert_eq!(names(&d.content.root), ["a", "b", "c", "d"]);
    }

    #[test]
    fn moving_into_a_group_and_rejecting_cycles() {
        let mut d = doc_with(&["a", "b", "c"]);
        let ids: Vec<u64> = d.content.root.iter().map(|n| n.id).collect();
        new_group(&mut d, "g".into(), &[ids[1]]).unwrap();
        let g = d.content.root[1].id;
        move_layers(&mut d, &[ids[2]], Some(g), 0).unwrap();
        assert_eq!(names(d.content.root[1].children().unwrap()), ["c", "b"]);
        assert!(move_layers(&mut d, &[g], Some(g), 0).is_err());
        // reorder at top level: move "a" to the top
        move_layers(&mut d, &[ids[0]], None, 2).unwrap();
        assert_eq!(names(&d.content.root), ["g", "a"]);
    }

    #[test]
    fn duplicate_shares_pixels_until_edited() {
        let mut d = doc_with(&["a"]);
        let id = d.content.root[0].id;
        duplicate(&mut d, &[id], " copy").unwrap();
        assert_eq!(names(&d.content.root), ["a", "a copy"]);
        let (t0, t1) = (d.content.root[0].tiles().unwrap(), d.content.root[1].tiles().unwrap());
        assert!(Arc::ptr_eq(t0, t1), "no pixel copy until one of them is edited");
        assert_ne!(d.content.root[0].id, d.content.root[1].id);
    }

    #[test]
    fn delete_keeps_at_least_one_layer() {
        let mut d = doc_with(&["a", "b"]);
        let ids: Vec<u64> = d.content.root.iter().map(|n| n.id).collect();
        assert!(delete(&mut d, &ids).is_err());
        delete(&mut d, &[ids[1]]).unwrap();
        assert_eq!(names(&d.content.root), ["a"]);
        assert_eq!(d.content.active, ids[0]);
    }

    #[test]
    fn transient_changes_skip_history_until_committed() {
        let mut d = doc_with(&["a"]);
        let id = d.content.root[0].id;
        let steps = d.history.states.len();
        for v in [0.9, 0.7, 0.5] {
            set_props(&mut d, id, LayerProps { opacity: Some(v), ..Default::default() }, true).unwrap();
        }
        assert_eq!(d.history.states.len(), steps);
        set_props(&mut d, id, LayerProps { opacity: Some(0.4), ..Default::default() }, false).unwrap();
        assert_eq!(d.history.states.len(), steps + 1);
        d.undo();
        assert_eq!(d.content.root[0].opacity, 1.0);
    }

    #[test]
    fn merge_down_matches_the_composite() {
        let mut d = doc_with(&["a", "b"]);
        let top = &mut d.content.root[1];
        top.kind = NodeKind::Pixel { tiles: Arc::new(filled_tiles(256, 256, [200, 50, 50, 128])) };
        top.blend = BlendMode::Multiply;
        top.opacity = 0.7;
        let before = composite_lod0(&d.content, 0, 0).unwrap();
        let id = d.content.root[1].id;
        merge_down(&mut d, id).unwrap();
        assert_eq!(d.content.root.len(), 1);
        let after = composite_lod0(&d.content, 0, 0).unwrap();
        for (a, b) in before.0.iter().zip(after.0.iter()) {
            assert!((*a as i32 - *b as i32).abs() <= 1);
        }
    }

    #[test]
    fn history_drops_old_steps_past_the_limit() {
        let mut d = doc_with(&["a"]);
        d.history.max_steps = 5;
        let id = d.content.root[0].id;
        for i in 0..20 {
            set_props(&mut d, id, LayerProps { name: Some(format!("n{i}")), ..Default::default() }, false).unwrap();
        }
        assert_eq!(d.history.states.len(), 6);
        while d.undo() {}
        assert_eq!(d.content.root[0].name, "n14");
    }
}
