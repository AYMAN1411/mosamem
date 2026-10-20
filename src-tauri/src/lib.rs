mod ai;
mod codecs;
mod commands;
mod doc;
mod fsx;
#[cfg(any(debug_assertions, feature = "harness"))]
mod harness;
mod persist;
mod pressure;
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
            pressure::install(app.handle());
            app.manage(recovery);
            app.manage(std::sync::Arc::new(ai::Ai::new(data.join("models"))));
            #[cfg(any(debug_assertions, feature = "harness"))]
            harness::start(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ai_status,
            commands::ai_download,
            commands::ai_cancel,
            commands::ai_delete,
            commands::ai_select_subject,
            commands::ai_remove_background,
            commands::ai_object_prepare,
            commands::ai_object_select,
            commands::ai_smart_fill,
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
            commands::adjustment_new,
            commands::adjustment_set,
            commands::histogram,
            commands::adjustment_compile,
            commands::live_filter,
            commands::filter_apply,
            commands::layer_transform,
            commands::transform_bounds,
            commands::image_size,
            commands::canvas_size,
            commands::crop,
            commands::turn_image,
            commands::live_begin_transform,
            commands::text_new,
            commands::text_set,
            commands::layer_rasterize,
            commands::text_metrics,
            commands::layer_style_set,
            commands::read_file,
            commands::write_file,
            commands::psd_import,
            commands::psd_export,
            commands::layer_solo,
            commands::layer_move,
            commands::layer_merge_down,
            commands::layer_merge_visible,
            commands::layer_flatten,
            commands::set_active,
            commands::select_shape,
            commands::select_wand,
            commands::select_quick,
            commands::select_simple,
            commands::select_modify,
            commands::select_translate,
            commands::select_hit,
            commands::refine_preview,
            commands::refine_cancel,
            commands::refine_apply,
            commands::mask_add,
            commands::mask_delete,
            commands::mask_apply,
            commands::mask_set,
            commands::mask_to_selection,
            commands::paint_begin,
            commands::paint_points,
            commands::paint_end,
            commands::paint_bucket,
            commands::paint_gradient,
            commands::sample_color,
            commands::live_begin,
            commands::live_begin_full,
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
            commands::debug_adjust_reference,
            #[cfg(any(debug_assertions, feature = "harness"))]
            commands::debug_flatten,
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
