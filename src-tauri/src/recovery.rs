//! Autosave and crash recovery.
//!
//! Every open document with unsaved changes is written to `<app data>/recovery/<session>-<doc>.mosamem`
//! about 2 s after the user pauses, and at least every 20 s while they keep working. Writes are
//! incremental (only new tiles) and transactional (SQLite), on a background thread that never blocks
//! editing. A lock file marks the running session; if the app finds a lock from a session whose
//! process is gone, that session crashed, and its recovery files are offered on launch.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::Serialize;

use crate::doc::{DocId, Engine};
use crate::persist::{self, HashCache};

pub const IDLE: Duration = Duration::from_secs(2);
pub const MAX_INTERVAL: Duration = Duration::from_secs(20);

struct Tracked {
    file: PathBuf,
    saved_counter: u64,
    seen_counter: u64,
    seen_at: Instant,
    last_save: Instant,
}

pub struct Recovery {
    pub dir: PathBuf,
    pub session: String,
    hashes: HashCache,
    docs: Mutex<HashMap<DocId, Tracked>>,
    /// Sessions that ended without a clean exit, found at startup.
    pub crashed: Mutex<Vec<String>>,
    pool: rayon::ThreadPool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveredFile {
    pub file: String,
    pub name: String,
    pub original_path: Option<String>,
    pub saved_at: u64,
    pub width: u32,
    pub height: u32,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    // signal 0 only checks that the process exists
    unsafe { libc_kill(pid as i32, 0) == 0 }
}
#[cfg(unix)]
unsafe extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}
/// Autosave runs at background priority so it never competes with painting and other edits.
fn lower_priority() {
    #[cfg(target_os = "macos")]
    unsafe {
        // QOS_CLASS_UTILITY
        pthread_set_qos_class_self_np(0x11, 0);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    unsafe {
        nice(10);
    }
    #[cfg(windows)]
    unsafe {
        // THREAD_PRIORITY_BELOW_NORMAL
        SetThreadPriority(GetCurrentThread(), -1);
    }
}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn pthread_set_qos_class_self_np(class: u32, relative_priority: i32) -> i32;
}
#[cfg(all(unix, not(target_os = "macos")))]
unsafe extern "C" {
    fn nice(inc: i32) -> i32;
}
#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThread() -> *mut std::ffi::c_void;
    fn SetThreadPriority(thread: *mut std::ffi::c_void, priority: i32) -> i32;
}

#[cfg(windows)]
fn pid_alive(pid: u32) -> bool {
    use std::process::Command;
    Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/NH"]).output().is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
}

impl Recovery {
    pub fn new(app_data: &Path) -> Arc<Recovery> {
        let dir = app_data.join("recovery");
        let _ = std::fs::create_dir_all(&dir);
        let session = format!("{:x}{:04x}", now_secs(), std::process::id() & 0xffff);
        let mut crashed = Vec::new();
        // locks left by sessions whose process is gone → those sessions crashed
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("lock") {
                    continue;
                }
                let text = std::fs::read_to_string(&p).unwrap_or_default();
                let pid: u32 = text.trim().parse().unwrap_or(0);
                if pid == 0 || !pid_alive(pid) {
                    if let Some(s) = p.file_stem().and_then(|s| s.to_str()) {
                        crashed.push(s.to_string());
                    }
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
        let _ = std::fs::write(dir.join(format!("{session}.lock")), std::process::id().to_string());
        // a small pool of its own: hashing and compressing a large document must not take the
        // compute threads that painting and compositing share
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get() / 4).clamp(1, 4);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("autosave-{i}"))
            .start_handler(|_| lower_priority())
            .build()
            .expect("autosave pool");
        Arc::new(Recovery { dir, session, hashes: HashCache::default(), docs: Mutex::new(HashMap::new()), crashed: Mutex::new(crashed), pool })
    }

    pub fn hashes(&self) -> &HashCache {
        &self.hashes
    }

    fn file_for(&self, doc: DocId) -> PathBuf {
        self.dir.join(format!("{}-{doc}.mosamem", self.session))
    }

    /// One autosave pass: writes every document whose changes have settled.
    pub fn tick(&self, engine: &Engine) {
        for h in engine.all() {
            let (id, counter, dirty) = {
                let d = h.doc.read();
                (d.id, d.change_counter, d.dirty())
            };
            let now = Instant::now();
            let due = {
                let mut docs = self.docs.lock();
                let t = docs.entry(id).or_insert_with(|| Tracked { file: self.file_for(id), saved_counter: 0, seen_counter: 0, seen_at: now, last_save: now });
                if t.seen_counter != counter {
                    t.seen_counter = counter;
                    t.seen_at = now;
                }
                dirty && t.saved_counter != counter && (now - t.seen_at >= IDLE || now - t.last_save >= MAX_INTERVAL)
            };
            if !due {
                continue;
            }
            // snapshot under a short read lock; the write happens without holding it
            let (content, name, path) = {
                let d = h.doc.read();
                (d.content.clone(), d.name.clone(), d.path.clone())
            };
            let file = self.file_for(id);
            let extra = vec![
                ("recovery", "1".to_string()),
                ("session", self.session.clone()),
                ("savedAt", now_secs().to_string()),
                ("originalPath", path.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()),
            ];
            match self.pool.install(|| persist::save(&file, &content, &name, &extra, &self.hashes)) {
                Ok(()) => {
                    let mut docs = self.docs.lock();
                    if let Some(t) = docs.get_mut(&id) {
                        t.saved_counter = counter;
                        t.last_save = Instant::now();
                    }
                }
                Err(e) => eprintln!("[autosave] {name}: {e}"),
            }
        }
    }

    /// The document was closed or saved cleanly: its recovery copy is no longer needed.
    pub fn forget(&self, doc: DocId) {
        if let Some(t) = self.docs.lock().remove(&doc) {
            let _ = std::fs::remove_file(&t.file);
        }
    }

    /// Recovery files left by crashed sessions.
    pub fn list(&self) -> Vec<RecoveredFile> {
        let crashed = self.crashed.lock().clone();
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return out };
        for e in entries.flatten() {
            let p = e.path();
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if p.extension().and_then(|x| x.to_str()) != Some("mosamem") || !crashed.iter().any(|s| stem.starts_with(&format!("{s}-"))) {
                continue;
            }
            let Ok(meta) = persist::read_meta(&p) else { continue };
            let get = |k: &str| meta.get(k).cloned().unwrap_or_default();
            out.push(RecoveredFile {
                file: p.to_string_lossy().into_owned(),
                name: get("name"),
                original_path: Some(get("originalPath")).filter(|s| !s.is_empty()),
                saved_at: get("savedAt").parse().unwrap_or(0),
                width: get("width").parse().unwrap_or(0),
                height: get("height").parse().unwrap_or(0),
            });
        }
        out.sort_by_key(|r| std::cmp::Reverse(r.saved_at));
        out
    }

    pub fn discard(&self, file: &Path) {
        if file.starts_with(&self.dir) {
            let _ = std::fs::remove_file(file);
        }
    }

    /// Clean exit: remove this session's lock and recovery files.
    pub fn shutdown(&self) {
        let prefix = format!("{}-", self.session);
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with(&prefix) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        let _ = std::fs::remove_file(self.dir.join(format!("{}.lock", self.session)));
    }

    pub fn spawn<R: tauri::Runtime>(self: &Arc<Self>, app: tauri::AppHandle<R>) {
        use tauri::Manager;
        let me = self.clone();
        std::thread::Builder::new()
            .name("autosave".into())
            .spawn(move || {
                lower_priority();
                loop {
                    std::thread::sleep(Duration::from_millis(500));
                    me.tick(&app.state::<Engine>());
                }
            })
            .expect("autosave thread");
    }
}
