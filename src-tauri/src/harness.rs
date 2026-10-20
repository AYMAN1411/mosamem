//! Debug-only test harness: with MOSAMEM_HARNESS=1, a local HTTP endpoint on 127.0.0.1 evaluates
//! JavaScript in the main window and returns the result. Used by tools/harness.mjs for scripted UI
//! tests, screenshots and performance runs. Not compiled into release builds unless the `harness` feature is on.

use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime};

pub const PORT: u16 = 47811;

static PENDING: Mutex<Option<HashMap<u64, Sender<String>>>> = Mutex::new(None);

#[tauri::command]
pub fn harness_reply(id: u64, result: String) {
    if let Some(tx) = PENDING.lock().unwrap().as_mut().and_then(|m| m.remove(&id)) {
        let _ = tx.send(result);
    }
}

pub fn start<R: Runtime>(app: &AppHandle<R>) {
    if std::env::var("MOSAMEM_HARNESS").as_deref() != Ok("1") {
        return;
    }
    *PENDING.lock().unwrap() = Some(HashMap::new());
    let app = app.clone();
    std::thread::spawn(move || {
        let server = tiny_http::Server::http(("127.0.0.1", PORT)).expect("harness port busy");
        eprintln!("[harness] listening on 127.0.0.1:{PORT}");
        let mut next = 1u64;
        for mut req in server.incoming_requests() {
            let mut body = String::new();
            let _ = req.as_reader().read_to_string(&mut body);
            let id = next;
            next += 1;
            let (tx, rx) = channel();
            PENDING.lock().unwrap().as_mut().unwrap().insert(id, tx);
            // The script is the body of an async function; its return value is sent back as JSON.
            let js = format!(
                "(async () => {{ let r; try {{ r = {{ ok: true, value: await (async () => {{ {body} }})() }}; }} catch (e) {{ r = {{ ok: false, error: String(e && e.stack || e) }}; }} window.__TAURI_INTERNALS__.invoke('harness_reply', {{ id: {id}, result: JSON.stringify(r) ?? 'null' }}); }})();"
            );
            let reply = match app.get_webview_window("main") {
                Some(w) => match w.eval(&js) {
                    Ok(()) => rx.recv_timeout(Duration::from_secs(120)).unwrap_or_else(|_| r#"{"ok":false,"error":"timeout"}"#.into()),
                    Err(e) => format!(r#"{{"ok":false,"error":{:?}}}"#, e.to_string()),
                },
                None => r#"{"ok":false,"error":"no main window"}"#.into(),
            };
            let _ = req.respond(tiny_http::Response::from_string(reply));
        }
    });
}
