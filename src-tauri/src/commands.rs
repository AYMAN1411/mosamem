//! Commands the UI calls.

use std::path::PathBuf;

use serde::Deserialize;
use tauri::State;

use crate::codecs::{self, EncodeOptions, Format};
use crate::doc::{composite, next_id, DocInfo, Document, Engine, Layer};

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

#[tauri::command]
pub fn new_document(engine: State<'_, Engine>, spec: NewDocument) -> Res<DocInfo> {
    if spec.width == 0 || spec.height == 0 || spec.width > MAX_SIDE || spec.height > MAX_SIDE {
        return Err(format!("size must be between 1 and {MAX_SIDE} px"));
    }
    let layer = match spec.background {
        Background::White => Layer::filled(spec.background_layer_name, spec.width, spec.height, [255, 255, 255, 255]),
        Background::Black => Layer::filled(spec.background_layer_name, spec.width, spec.height, [0, 0, 0, 255]),
        Background::Color(c) => Layer::filled(spec.background_layer_name, spec.width, spec.height, c),
        Background::Transparent => Layer::new(spec.background_layer_name),
    };
    let doc = Document {
        id: next_id(),
        name: spec.name,
        path: None,
        width: spec.width,
        height: spec.height,
        ppi: spec.ppi,
        active: layer.id,
        layers: vec![layer],
        version: 1,
    };
    Ok(engine.insert(doc).doc.read().info())
}

#[tauri::command]
pub async fn open_image(engine: State<'_, Engine>, path: PathBuf, layer_name: String) -> Res<DocInfo> {
    let p = path.clone();
    let decoded = tauri::async_runtime::spawn_blocking(move || codecs::decode(&p)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    let layer = Layer::from_rgba(layer_name, decoded.width, decoded.height, &decoded.rgba);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let doc = Document {
        id: next_id(),
        name,
        path: Some(path),
        width: decoded.width,
        height: decoded.height,
        ppi: decoded.ppi.unwrap_or(72.0),
        active: layer.id,
        layers: vec![layer],
        version: 1,
    };
    Ok(engine.insert(doc).doc.read().info())
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
    let h = engine.get(spec.doc).ok_or("document not found")?;
    let format = spec.format.or_else(|| Format::from_path(&spec.path)).ok_or("unknown file type")?;
    tauri::async_runtime::spawn_blocking(move || {
        let ppi = h.doc.read().ppi;
        let (w, hgt, rgba) = composite::flatten(&h);
        let bytes = codecs::encode(format, w, hgt, &rgba, &EncodeOptions { quality: spec.quality, lossless: spec.lossless, ppi })
            .map_err(|e| e.to_string())?;
        crate::fsx::write_atomic(&spec.path, &bytes).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn close_document(engine: State<'_, Engine>, doc: u64) {
    engine.remove(doc);
}

#[tauri::command]
pub fn document_info(engine: State<'_, Engine>, doc: u64) -> Res<DocInfo> {
    Ok(engine.get(doc).ok_or("document not found")?.doc.read().info())
}

/// Debug builds only: a large synthetic document for the performance harness. Every layer covers
/// the whole canvas (worst case for compositing): a translucent gradient plus a denser disc.
#[cfg(any(debug_assertions, feature = "harness"))]
#[tauri::command]
pub fn debug_test_document(engine: State<'_, Engine>, width: u32, height: u32, layers: u32) -> DocInfo {
    use rayon::prelude::*;
    let mut stack = vec![Layer::filled("background", width, height, [235, 235, 240, 255])];
    stack.extend((1..layers).into_par_iter().map(|i| {
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
        Layer::from_rgba(format!("layer {i}"), width, height, &rgba)
    }).collect::<Vec<_>>());
    let doc = Document { id: next_id(), name: format!("perf {width}×{height}×{layers}"), path: None, width, height, ppi: 72.0, active: stack.last().unwrap().id, layers: stack, version: 1 };
    engine.insert(doc).doc.read().info()
}
