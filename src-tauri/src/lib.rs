mod codecs;
mod commands;
mod doc;
mod fsx;
#[cfg(any(debug_assertions, feature = "harness"))]
mod harness;
mod persist;
mod protocol;
mod recovery;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(doc::Engine::default())
        .register_asynchronous_uri_scheme_protocol("lawha", protocol::handle)
        .setup(|app| {
            use tauri::Manager;
            let data = app.path().app_data_dir()?;
            let recovery = recovery::Recovery::new(&data);
            recovery.spawn(app.handle().clone());
            app.manage(recovery);
            #[cfg(any(debug_assertions, feature = "harness"))]
            harness::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::new_document,
            commands::open_file,
            commands::save_document,
            commands::recovery_list,
            commands::recovery_open,
            commands::recovery_discard,
            commands::export_image,
            commands::close_document,
            commands::document_info,
            commands::layer_new,
            commands::layer_group,
            commands::layer_ungroup,
            commands::layer_duplicate,
            commands::layer_delete,
            commands::layer_set,
            commands::layer_solo,
            commands::layer_move,
            commands::layer_merge_down,
            commands::layer_merge_visible,
            commands::layer_flatten,
            commands::set_active,
            commands::live_begin,
            commands::live_end,
            commands::undo,
            commands::redo,
            commands::history_jump,
            #[cfg(any(debug_assertions, feature = "harness"))]
            commands::debug_test_document,
            #[cfg(any(debug_assertions, feature = "harness"))]
            commands::debug_doc_hash,
            #[cfg(any(debug_assertions, feature = "harness"))]
            commands::debug_blend_reference,
            #[cfg(any(debug_assertions, feature = "harness"))]
            harness::harness_reply,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Lawha")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                use tauri::Manager;
                // a clean exit: this session's recovery copies are no longer needed
                if let Some(r) = app.try_state::<std::sync::Arc<recovery::Recovery>>() {
                    r.shutdown();
                }
            }
        });
}
