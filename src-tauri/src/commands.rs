//! Commands the UI calls.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use tauri::State;

use crate::codecs::{self, EncodeOptions, Format};
use crate::persist;
use crate::recovery::{RecoveredFile, Recovery};
use crate::doc::ops::{self, LayerProps};
use crate::doc::{composite, filled_tiles, tiles_from_rgba, Content, DocHandle, DocInfo, Document, Engine, Label, Node, TileMap};

type Res<T> = Result<T, String>;

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase", tag = "kind", content = "rgba")]
pub enum Background {
    White,
    Black,
    Transparent,
    Color([u8; 4]),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDocument {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub ppi: f64,
    pub background: Background,
    /// Localized name of the bottom layer.
    pub background_layer_name: String,
}

pub const MAX_SIDE: u32 = 30_000;

fn handle(engine: &Engine, doc: u64) -> Res<Arc<DocHandle>> {
    engine.get(doc).ok_or_else(|| "document not found".into())
}

/// Runs an edit on a document and returns its new state.
fn edit(engine: &Engine, doc: u64, f: impl FnOnce(&mut Document) -> Res<()>) -> Res<DocInfo> {
    let h = handle(engine, doc)?;
    let mut d = h.doc.write();
    f(&mut d)?;
    Ok(d.info())
}

#[tauri::command]
pub fn new_document(engine: State<'_, Engine>, spec: NewDocument) -> Res<DocInfo> {
    if spec.width == 0 || spec.height == 0 || spec.width > MAX_SIDE || spec.height > MAX_SIDE {
        return Err(format!("size must be between 1 and {MAX_SIDE} px"));
    }
    let (w, h) = (spec.width, spec.height);
    let tiles = match spec.background {
        Background::White => filled_tiles(w, h, [255, 255, 255, 255]),
        Background::Black => filled_tiles(w, h, [0, 0, 0, 255]),
        Background::Color(c) => filled_tiles(w, h, c),
        Background::Transparent => TileMap::new(),
    };
    let layer = Node::pixel(spec.background_layer_name, tiles);
    let content = Content::new(w, h, spec.ppi, layer.id, vec![layer]);
    let doc = Document::new(spec.name, None, content, Label::new("history.new"));
    Ok(engine.insert(doc).doc.read().info())
}

/// Opens a `.lawha` document or a flat image.
#[tauri::command]
pub async fn open_file(engine: State<'_, Engine>, path: PathBuf, layer_name: String) -> Res<DocInfo> {
    let p = path.clone();
    if tauri::async_runtime::spawn_blocking(move || persist::is_lawha(&p)).await.map_err(|e| e.to_string())? {
        let p = path.clone();
        let loaded = tauri::async_runtime::spawn_blocking(move || persist::load(&p)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(loaded.name);
        let mut doc = Document::new(name, Some(path), loaded.content, Label::new("history.open"));
        doc.saved_at = Some(0);
        return Ok(engine.insert(doc).doc.read().info());
    }
    let p = path.clone();
    let decoded = tauri::async_runtime::spawn_blocking(move || codecs::decode(&p)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    let layer = Node::pixel(layer_name, tiles_from_rgba(decoded.width, decoded.height, &decoded.rgba));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let content = Content::new(decoded.width, decoded.height, decoded.ppi.unwrap_or(72.0), layer.id, vec![layer]);
    // an imported image has no Lawha file yet: it stays "unsaved" until saved as .lawha
    let doc = Document::new(name, Some(path), content, Label::new("history.open"));
    Ok(engine.insert(doc).doc.read().info())
}

/// Saves as `.lawha`. Saving over the document's own file updates it in place, transactionally;
/// any other target is written to a temporary file first and then moved into place.
#[tauri::command]
pub async fn save_document(engine: State<'_, Engine>, recovery: State<'_, Arc<Recovery>>, doc: u64, path: PathBuf) -> Res<DocInfo> {
    let h = handle(&engine, doc)?;
    let (content, index) = {
        let d = h.doc.read();
        (d.content.clone(), d.history.current)
    };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let target = path.clone();
    let hashes = recovery.inner().clone();
    let nm = name.clone();
    tauri::async_runtime::spawn_blocking(move || -> Res<()> {
        if persist::is_lawha(&target) {
            persist::save(&target, &content, &nm, &vec![], hashes.hashes()).map_err(|e| e.to_string())
        } else {
            let tmp = target.with_extension("lawha-saving");
            let _ = std::fs::remove_file(&tmp);
            persist::save(&tmp, &content, &nm, &vec![], hashes.hashes()).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, &target).map_err(|e| e.to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut d = h.doc.write();
    d.path = Some(path);
    d.name = name;
    d.saved_at = Some(index);
    d.touch();
    recovery.forget(d.id);
    Ok(d.info())
}

#[tauri::command]
pub fn recovery_list(recovery: State<'_, Arc<Recovery>>) -> Vec<RecoveredFile> {
    recovery.list()
}

/// Opens a recovered document. It stays unsaved (dirty) until the user saves it.
#[tauri::command]
pub async fn recovery_open(engine: State<'_, Engine>, recovery: State<'_, Arc<Recovery>>, file: PathBuf) -> Res<DocInfo> {
    if !file.starts_with(&recovery.dir) {
        return Err("not a recovery file".into());
    }
    let f = file.clone();
    let loaded = tauri::async_runtime::spawn_blocking(move || persist::load(&f)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    let original = loaded.meta.get("originalPath").filter(|s| !s.is_empty()).map(PathBuf::from);
    // only a real .lawha original can be saved over directly; an image original must be saved anew
    let path = original.filter(|p| p.extension().is_some_and(|e| e == "lawha"));
    let doc = Document::new(loaded.name, path, loaded.content, Label::new("history.recovered"));
    let info = engine.insert(doc).doc.read().info();
    recovery.discard(&file);
    Ok(info)
}

#[tauri::command]
pub fn recovery_discard(recovery: State<'_, Arc<Recovery>>, file: PathBuf) {
    recovery.discard(&file);
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSpec {
    pub doc: u64,
    pub path: PathBuf,
    pub format: Option<Format>,
    pub quality: u8,
    pub lossless: bool,
}

#[tauri::command]
pub async fn export_image(engine: State<'_, Engine>, spec: ExportSpec) -> Res<()> {
    let h = handle(&engine, spec.doc)?;
    let format = spec.format.or_else(|| Format::from_path(&spec.path)).ok_or("unknown file type")?;
    tauri::async_runtime::spawn_blocking(move || {
        let ppi = h.doc.read().content.ppi;
        let (w, hgt, rgba) = composite::flatten(&h);
        let bytes = codecs::encode(format, w, hgt, &rgba, &EncodeOptions { quality: spec.quality, lossless: spec.lossless, ppi }).map_err(|e| e.to_string())?;
        crate::fsx::write_atomic(&spec.path, &bytes).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn close_document(engine: State<'_, Engine>, recovery: State<'_, Arc<Recovery>>, doc: u64) {
    engine.remove(doc);
    recovery.forget(doc);
}

#[tauri::command]
pub fn document_info(engine: State<'_, Engine>, doc: u64) -> Res<DocInfo> {
    Ok(handle(&engine, doc)?.doc.read().info())
}

// ---- layers ----

#[tauri::command]
pub fn layer_new(engine: State<'_, Engine>, doc: u64, name: String) -> Res<DocInfo> {
    edit(&engine, doc, |d| ops::new_layer(d, name))
}

#[tauri::command]
pub fn layer_group(engine: State<'_, Engine>, doc: u64, name: String, ids: Vec<u64>) -> Res<DocInfo> {
    edit(&engine, doc, |d| ops::new_group(d, name, &ids))
}

#[tauri::command]
pub fn layer_ungroup(engine: State<'_, Engine>, doc: u64, id: u64) -> Res<DocInfo> {
    edit(&engine, doc, |d| ops::ungroup(d, id))
}

#[tauri::command]
pub fn layer_duplicate(engine: State<'_, Engine>, doc: u64, ids: Vec<u64>, suffix: String) -> Res<DocInfo> {
    edit(&engine, doc, |d| ops::duplicate(d, &ids, &suffix))
}

#[tauri::command]
pub fn layer_delete(engine: State<'_, Engine>, doc: u64, ids: Vec<u64>) -> Res<DocInfo> {
    edit(&engine, doc, |d| ops::delete(d, &ids))
}

#[tauri::command]
pub fn layer_set(engine: State<'_, Engine>, doc: u64, id: u64, props: LayerProps, transient: bool) -> Res<DocInfo> {
    edit(&engine, doc, |d| ops::set_props(d, id, props, transient))
}

#[tauri::command]
pub fn layer_solo(engine: State<'_, Engine>, doc: u64, id: u64) -> Res<DocInfo> {
    edit(&engine, doc, |d| ops::solo(d, id))
}

#[tauri::command]
pub fn layer_move(engine: State<'_, Engine>, doc: u64, ids: Vec<u64>, parent: Option<u64>, index: usize) -> Res<DocInfo> {
    edit(&engine, doc, |d| ops::move_layers(d, &ids, parent, index))
}

#[tauri::command]
pub fn layer_merge_down(engine: State<'_, Engine>, doc: u64, id: u64) -> Res<DocInfo> {
    edit(&engine, doc, |d| ops::merge_down(d, id))
}

#[tauri::command]
pub fn layer_merge_visible(engine: State<'_, Engine>, doc: u64, name: String) -> Res<DocInfo> {
    edit(&engine, doc, |d| ops::merge_visible(d, name))
}

#[tauri::command]
pub fn layer_flatten(engine: State<'_, Engine>, doc: u64, name: String) -> Res<DocInfo> {
    edit(&engine, doc, |d| ops::flatten(d, name))
}

#[tauri::command]
pub fn set_active(engine: State<'_, Engine>, doc: u64, id: u64) -> Res<DocInfo> {
    edit(&engine, doc, |d| {
        if d.content.find(id).is_none() {
            return Err("layer not found".into());
        }
        d.content.active = id;
        Ok(())
    })
}

// ---- selections ----

use crate::doc::selection::{self, MaskProps, Modify, NewMask, RefineOutput, Shape};
use lawha_core::select::{Combine, Refine};

#[tauri::command]
pub async fn select_shape(engine: State<'_, Engine>, doc: u64, shape: Shape, mode: Combine, feather: f32) -> Res<DocInfo> {
    let h = handle(&engine, doc)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = h.doc.write();
        selection::shape(&mut d, shape, mode, feather)?;
        Ok(d.info())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WandSpec {
    pub x: f64,
    pub y: f64,
    pub tolerance: u8,
    pub contiguous: bool,
    pub all_layers: bool,
    pub antialias: bool,
    pub mode: Combine,
}

#[tauri::command]
pub async fn select_wand(engine: State<'_, Engine>, doc: u64, spec: WandSpec) -> Res<DocInfo> {
    let h = handle(&engine, doc)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = h.doc.write();
        selection::wand(&mut d, spec.x, spec.y, spec.tolerance, spec.contiguous, spec.all_layers, spec.antialias, spec.mode)?;
        Ok(d.info())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn select_quick(engine: State<'_, Engine>, doc: u64, points: Vec<(f64, f64)>, radius: f64, all_layers: bool, subtract: bool) -> Res<DocInfo> {
    let h = handle(&engine, doc)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = h.doc.write();
        selection::quick(&mut d, &points, radius, all_layers, subtract)?;
        Ok(d.info())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectSimple {
    All,
    Deselect,
    Reselect,
    Invert,
}

#[tauri::command]
pub fn select_simple(engine: State<'_, Engine>, doc: u64, op: SelectSimple) -> Res<DocInfo> {
    edit(&engine, doc, |d| match op {
        SelectSimple::All => selection::all(d),
        SelectSimple::Deselect => selection::deselect(d),
        SelectSimple::Reselect => selection::reselect(d),
        SelectSimple::Invert => selection::invert(d),
    })
}

#[tauri::command]
pub async fn select_modify(engine: State<'_, Engine>, doc: u64, op: Modify) -> Res<DocInfo> {
    let h = handle(&engine, doc)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = h.doc.write();
        selection::modify(&mut d, op)?;
        Ok(d.info())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Is the document point inside the selection (≥ 50%)?
#[tauri::command]
pub fn select_hit(engine: State<'_, Engine>, doc: u64, x: f64, y: f64) -> Res<bool> {
    let h = handle(&engine, doc)?;
    let d = h.doc.read();
    let (w, ht) = (d.content.width as f64, d.content.height as f64);
    let Some(m) = d.content.selection.as_ref() else { return Ok(false) };
    if x < 0.0 || y < 0.0 || x >= w || y >= ht {
        return Ok(false);
    }
    let (xi, yi) = (x as i32, y as i32);
    let t = lawha_core::tile::TILE as i32;
    Ok(m.tile((xi / t, yi / t)).map_or(m.outside, |tile| tile.get((xi % t) as usize, (yi % t) as usize)) >= 128)
}

#[tauri::command]
pub fn select_translate(engine: State<'_, Engine>, doc: u64, dx: i32, dy: i32) -> Res<DocInfo> {
    edit(&engine, doc, |d| selection::translate(d, dx, dy))
}

#[tauri::command]
pub async fn refine_preview(engine: State<'_, Engine>, doc: u64, params: Refine) -> Res<DocInfo> {
    let h = handle(&engine, doc)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = h.doc.write();
        selection::refine_preview(&mut d, &params)?;
        Ok(d.info())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn refine_cancel(engine: State<'_, Engine>, doc: u64) -> Res<DocInfo> {
    edit(&engine, doc, |d| {
        selection::refine_cancel(d);
        Ok(())
    })
}

#[tauri::command]
pub async fn refine_apply(engine: State<'_, Engine>, doc: u64, params: Refine, output: RefineOutput, decontaminate: bool, layer_name: String) -> Res<DocInfo> {
    let h = handle(&engine, doc)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = h.doc.write();
        selection::refine_apply(&mut d, &params, output, decontaminate, &layer_name)?;
        Ok(d.info())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- layer masks ----

#[tauri::command]
pub fn mask_add(engine: State<'_, Engine>, doc: u64, id: u64, kind: NewMask) -> Res<DocInfo> {
    edit(&engine, doc, |d| selection::mask_add(d, id, kind))
}

#[tauri::command]
pub fn mask_delete(engine: State<'_, Engine>, doc: u64, id: u64) -> Res<DocInfo> {
    edit(&engine, doc, |d| selection::mask_delete(d, id))
}

#[tauri::command]
pub fn mask_apply(engine: State<'_, Engine>, doc: u64, id: u64) -> Res<DocInfo> {
    edit(&engine, doc, |d| selection::mask_apply(d, id))
}

#[tauri::command]
pub fn mask_set(engine: State<'_, Engine>, doc: u64, id: u64, props: MaskProps, transient: bool) -> Res<DocInfo> {
    edit(&engine, doc, |d| selection::mask_set(d, id, props, transient))
}

#[tauri::command]
pub fn mask_to_selection(engine: State<'_, Engine>, doc: u64, id: u64) -> Res<DocInfo> {
    edit(&engine, doc, |d| selection::mask_to_selection(d, id))
}

// ---- painting ----

use crate::doc::paint::{self, BucketSpec, GradientSpec, StrokeSpec};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrokeStarted {
    pub stroke: u64,
    /// GPU live session showing the stroke, if the layer allows one.
    pub live: Option<u64>,
}

#[tauri::command]
pub async fn paint_begin(engine: State<'_, Engine>, doc: u64, spec: StrokeSpec) -> Res<StrokeStarted> {
    let h = handle(&engine, doc)?;
    let layer = spec.layer;
    let e: &Engine = &engine;
    let stroke = {
        let d = h.doc.read();
        paint::begin(&d, spec)?
    };
    let live = e.live_begin(doc, layer);
    let id = crate::doc::next_id();
    e.strokes.map.lock().insert(id, stroke);
    Ok(StrokeStarted { stroke: id, live })
}

/// Adds pointer samples `(x, y, pressure)`; returns the lod-0 tiles that changed.
#[tauri::command]
pub async fn paint_points(engine: State<'_, Engine>, stroke: u64, live: Option<u64>, points: Vec<(f32, f32, f32)>) -> Res<Vec<(i32, i32)>> {
    let e: &Engine = &engine;
    let mut strokes = e.strokes.map.lock();
    let s = strokes.get_mut(&stroke).ok_or("stroke not found")?;
    let h = handle(e, s.doc)?;
    let mut d = h.doc.write();
    let keys = s.add(&mut d, &points);
    match live.and_then(|id| e.live(id)) {
        Some(session) => {
            if let Some(n) = d.content.find(s.spec.layer) {
                session.refresh_layer(n, &keys);
            }
        }
        None => h.composite.invalidate(&keys),
    }
    Ok(keys)
}

#[tauri::command]
pub async fn paint_end(engine: State<'_, Engine>, stroke: u64) -> Res<DocInfo> {
    let e: &Engine = &engine;
    let s = e.strokes.map.lock().remove(&stroke).ok_or("stroke not found")?;
    let h = handle(e, s.doc)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = h.doc.write();
        s.end(&mut d);
        Ok(d.info())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn paint_bucket(engine: State<'_, Engine>, doc: u64, spec: BucketSpec) -> Res<DocInfo> {
    let h = handle(&engine, doc)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = h.doc.write();
        paint::bucket(&mut d, spec)?;
        Ok(d.info())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn paint_gradient(engine: State<'_, Engine>, doc: u64, spec: GradientSpec) -> Res<DocInfo> {
    let h = handle(&engine, doc)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut d = h.doc.write();
        paint::gradient(&mut d, spec)?;
        Ok(d.info())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn sample_color(engine: State<'_, Engine>, doc: u64, x: f64, y: f64, size: u32, all_layers: bool) -> Res<Option<[u8; 4]>> {
    let h = handle(&engine, doc)?;
    tauri::async_runtime::spawn_blocking(move || Ok(paint::sample_color(&h.doc.read(), x, y, size, all_layers)))
        .await
        .map_err(|e| e.to_string())?
}

// ---- live sessions (GPU previews) ----

#[tauri::command]
pub fn live_begin(engine: State<'_, Engine>, doc: u64, layer: u64) -> Option<u64> {
    engine.live_begin(doc, layer)
}

#[tauri::command]
pub fn live_end(engine: State<'_, Engine>, session: u64) {
    engine.live_end(session);
}

// ---- history ----

#[tauri::command]
pub fn undo(engine: State<'_, Engine>, doc: u64) -> Res<DocInfo> {
    edit(&engine, doc, |d| {
        d.undo();
        Ok(())
    })
}

#[tauri::command]
pub fn redo(engine: State<'_, Engine>, doc: u64) -> Res<DocInfo> {
    edit(&engine, doc, |d| {
        d.redo();
        Ok(())
    })
}

#[tauri::command]
pub fn history_jump(engine: State<'_, Engine>, doc: u64, index: usize) -> Res<DocInfo> {
    edit(&engine, doc, |d| {
        d.jump(index);
        Ok(())
    })
}

/// Debug builds only: BLAKE3 of the flattened image, for comparing documents in tests.
#[cfg(any(debug_assertions, feature = "harness"))]
#[tauri::command]
pub fn debug_doc_hash(engine: State<'_, Engine>, doc: u64) -> Res<String> {
    let h = handle(&engine, doc)?;
    let (w, ht, rgba) = composite::flatten(&h);
    Ok(format!("{w}x{ht}:{}", blake3::hash(&rgba).to_hex()))
}

/// Debug builds only: inputs and CPU result for the GPU-vs-CPU blend test (3 × 256×256 RGBA8).
#[cfg(any(debug_assertions, feature = "harness"))]
#[tauri::command]
pub fn debug_blend_reference(mode: lawha_core::BlendMode, opacity: f32) -> tauri::ipc::Response {
    use lawha_core::tile::{Accum, Tile};
    let mut back = Tile::transparent();
    let mut src = Tile::transparent();
    for (i, (b, s)) in back.0.chunks_exact_mut(4).zip(src.0.chunks_exact_mut(4)).enumerate() {
        let (x, y) = (i % 256, i / 256);
        // every 8-bit value along the axes, a band of partial alpha, and transparent corners
        b.copy_from_slice(&[x as u8, (y * 3 % 256) as u8, ((x ^ y) % 256) as u8, if y < 32 { (x % 256) as u8 } else { 255 }]);
        s.copy_from_slice(&[y as u8, (x * 5 % 256) as u8, ((x * 7 + y) % 256) as u8, if x < 24 { 0 } else if y > 224 { (y * 2 % 256) as u8 } else { 255 }]);
    }
    let mut acc = Accum::from_tile(&back, 1.0);
    acc.composite(&src, mode, opacity, (0, 0));
    let mut out = Tile::transparent();
    acc.to_tile(&mut out);
    let mut bytes = Vec::with_capacity(3 * back.0.len());
    bytes.extend_from_slice(&back.0);
    bytes.extend_from_slice(&src.0);
    bytes.extend_from_slice(&out.0);
    tauri::ipc::Response::new(bytes)
}

/// Debug builds only: a large synthetic document for the performance harness. Every layer covers
/// the whole canvas (worst case for compositing): a translucent gradient plus a denser disc.
#[cfg(any(debug_assertions, feature = "harness"))]
#[tauri::command]
pub fn debug_test_document(engine: State<'_, Engine>, width: u32, height: u32, layers: u32) -> DocInfo {
    use rayon::prelude::*;
    let mut stack = vec![Node::pixel("background", filled_tiles(width, height, [235, 235, 240, 255]))];
    stack.extend(
        (1..layers)
            .into_par_iter()
            .map(|i| {
                let mut rgba = vec![0u8; (width * height * 4) as usize];
                let (cx, cy) = ((i * 1931 % 97) as f32 / 97.0 * width as f32, (i * 3571 % 89) as f32 / 89.0 * height as f32);
                let r = (0.15 + (i * 7 % 10) as f32 / 40.0) * width.min(height) as f32;
                rgba.par_chunks_mut((width * 4) as usize).enumerate().for_each(|(y, row)| {
                    for x in 0..width as usize {
                        let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
                        let a = if d < r { 120.0 + 135.0 * (1.0 - d / r) } else { 18.0 + 30.0 * (x as f32 / width as f32) };
                        row[x * 4..x * 4 + 4].copy_from_slice(&[(i * 47 % 255) as u8, (x / 16 % 255) as u8, (y / 16 % 255) as u8, a as u8]);
                    }
                });
                Node::pixel(format!("layer {i}"), tiles_from_rgba(width, height, &rgba))
            })
            .collect::<Vec<_>>(),
    );
    let content = Content::new(width, height, 72.0, stack.last().unwrap().id, stack);
    let doc = Document::new(format!("perf {width}×{height}×{layers}"), None, content, Label::new("history.new"));
    engine.insert(doc).doc.read().info()
}
