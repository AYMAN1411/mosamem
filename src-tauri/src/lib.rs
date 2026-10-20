mod spike;

pub fn run() {
    tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("tile", spike::tile_protocol)
        .invoke_handler(tauri::generate_handler![
            spike::spike_report,
            spike::spike_upload,
            spike::spike_download,
            spike::spike_exit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lawha");
}
