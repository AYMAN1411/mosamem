//! Milestone 0 measurement hooks. Removed once the spikes are written up.

use std::io::Write;
use tauri::http::{Response, StatusCode};
use tauri::ipc::{InvokeBody, Request};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder, Wry};

/// Serves synthetic 256×256 RGBA tiles: `tile://localhost/<n>`.
pub fn tile_protocol(
    _ctx: UriSchemeContext<'_, Wry>,
    request: tauri::http::Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let n: u32 = request.uri().path().trim_start_matches('/').parse().unwrap_or(0);
    std::thread::spawn(move || {
        let mut body = vec![0u8; 256 * 256 * 4];
        for (i, px) in body.chunks_exact_mut(4).enumerate() {
            let v = (i as u32).wrapping_mul(2654435761).wrapping_add(n) as u8;
            px.copy_from_slice(&[v, v ^ 0x5a, v.wrapping_add(n as u8), 255]);
        }
        let response = Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/octet-stream")
            .header("Access-Control-Allow-Origin", "*")
            .header("Cross-Origin-Resource-Policy", "cross-origin")
            .body(body)
            .unwrap();
        responder.respond(response);
    });
}

#[tauri::command]
pub fn spike_report<R: tauri::Runtime>(app: tauri::AppHandle<R>, name: String, json: String) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../spikes/out");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("{name}.json"));
    if let Ok(mut f) = std::fs::File::create(&path) {
        let _ = f.write_all(json.as_bytes());
    }
    println!("[spike] {name}: {json}");
    let _ = app.get_webview_window("main");
}

#[tauri::command]
pub fn spike_upload(request: Request<'_>) -> usize {
    match request.body() {
        InvokeBody::Raw(bytes) => bytes.len(),
        InvokeBody::Json(_) => 0,
    }
}

#[tauri::command]
pub fn spike_download(len: usize) -> tauri::ipc::Response {
    tauri::ipc::Response::new(vec![7u8; len])
}

#[tauri::command]
pub fn spike_exit<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    app.exit(0);
}
