mod codecs;
mod commands;
mod doc;
mod fsx;
#[cfg(any(debug_assertions, feature = "harness"))]
mod harness;
mod protocol;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(doc::Engine::default())
        .register_asynchronous_uri_scheme_protocol("lawha", protocol::handle)
        .setup(|_app| {
            #[cfg(any(debug_assertions, feature = "harness"))]
            harness::start(_app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::new_document,
            commands::open_image,
            commands::export_image,
            commands::close_document,
            commands::document_info,
            #[cfg(any(debug_assertions, feature = "harness"))]
            commands::debug_test_document,
            #[cfg(any(debug_assertions, feature = "harness"))]
            harness::harness_reply,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lawha");
}
