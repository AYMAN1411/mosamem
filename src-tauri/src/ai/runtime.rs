//! Running the models: ONNX Runtime with its WebGPU provider (Metal on the Mac, Direct3D 12 on
//! Windows), falling back to the CPU when the GPU path can't load or run a model. Sessions stay
//! loaded after first use. ONNX Runtime's automatic device choice is turned off: on the Mac it
//! would pick Core ML, which failed or was slower for these models (docs/status.md, M10).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use ort::ep;
use ort::environment::Environment;
use ort::session::{builder::AutoDevicePolicy, Session};
use parking_lot::Mutex;

pub type AiResult<T> = Result<T, String>;

fn env() -> AiResult<&'static Environment> {
    static ENV: OnceLock<Result<Environment, String>> = OnceLock::new();
    ENV.get_or_init(|| ort::init().with_name("lawha").with_telemetry(false).build().map_err(|e| e.to_string())).as_ref().map_err(|e| e.clone())
}

/// `LAWHA_AI_DEVICE=cpu` forces the CPU (for comparisons and tests).
fn gpu_allowed() -> bool {
    std::env::var("LAWHA_AI_DEVICE").map_or(true, |v| v != "cpu")
}

fn load(path: &Path, gpu: bool) -> AiResult<Session> {
    // no busy-waiting threads between runs: they would hold the CPU cores the GPU path and the rest
    // of the app need (and burn power while the user isn't doing anything)
    let mut b = Session::builder(env()?)
        .map_err(|e| e.to_string())?
        .with_auto_device(AutoDevicePolicy::PreferCPU)
        .map_err(|e| e.to_string())?
        .with_intra_op_spinning(false)
        .map_err(|e| e.to_string())?
        .with_inter_op_spinning(false)
        .map_err(|e| e.to_string())?;
    if gpu {
        b = b.with_execution_providers([ep::WebGPU::default().build().error_on_failure()]).map_err(|e| e.to_string())?;
    }
    b.commit_from_file(path).map_err(|e| e.to_string())
}

pub struct Loaded {
    pub session: Session,
    pub gpu: bool,
}

/// Sessions are never destroyed while Lawha runs: tearing one down crashed once in three test
/// runs, inside ONNX Runtime's own destructor (a malloc trap in ~InferenceSession on macOS 27, after
/// using the WebGPU provider). A replaced or unloaded session is parked here instead, and the
/// memory goes back to the system when the app quits.
fn park<T: Send + 'static>(v: T) {
    static PARKED: OnceLock<Mutex<Vec<Box<dyn Send>>>> = OnceLock::new();
    PARKED.get_or_init(Default::default).lock().push(Box::new(v));
}

#[derive(Default)]
pub struct Runtime {
    sessions: Mutex<HashMap<PathBuf, Arc<Mutex<Loaded>>>>,
    /// Models that failed on the GPU path, so they go straight to the CPU next time.
    cpu_only: Mutex<std::collections::HashSet<PathBuf>>,
    force_cpu: bool,
}

impl Runtime {
    /// A runtime that never uses the GPU (for comparing results).
    #[cfg(test)]
    pub fn cpu() -> Self {
        let mut r = Runtime::default();
        r.force_cpu = true;
        r
    }

    fn session(&self, path: &Path) -> AiResult<Arc<Mutex<Loaded>>> {
        if let Some(s) = self.sessions.lock().get(path) {
            return Ok(s.clone());
        }
        let want_gpu = !self.force_cpu && gpu_allowed() && !self.cpu_only.lock().contains(path);
        let loaded = match want_gpu.then(|| load(path, true)) {
            Some(Ok(s)) => Loaded { session: s, gpu: true },
            _ => {
                self.cpu_only.lock().insert(path.to_path_buf());
                Loaded { session: load(path, false)?, gpu: false }
            }
        };
        let s = Arc::new(Mutex::new(loaded));
        self.sessions.lock().insert(path.to_path_buf(), s.clone());
        Ok(s)
    }

    /// Runs `f` with the model's session. If it fails on the GPU, the model is reloaded on the CPU
    /// and `f` runs again.
    pub fn with<T>(&self, path: &Path, f: impl Fn(&mut Session) -> AiResult<T>) -> AiResult<T> {
        let s = self.session(path)?;
        let mut l = s.lock();
        match f(&mut l.session) {
            Ok(v) => Ok(v),
            Err(e) if l.gpu => {
                eprintln!("[ai] {} failed on the GPU ({e}); using the CPU", path.display());
                self.cpu_only.lock().insert(path.to_path_buf());
                let cpu = Loaded { session: load(path, false)?, gpu: false };
                park(std::mem::replace(&mut *l, cpu));
                f(&mut l.session)
            }
            Err(e) => Err(e),
        }
    }

    /// Whether the model last ran on the GPU (for the status report).
    /// Never waits for a model that's running (None then): this answers the status call, which
    /// must not block.
    pub fn on_gpu(&self, path: &Path) -> Option<bool> {
        self.sessions.lock().get(path).and_then(|s| s.try_lock().map(|l| l.gpu))
    }

    /// Lets go of the sessions of models under `dir` (before their files are deleted).
    pub fn unload(&self, dir: &Path) {
        let mut sessions = self.sessions.lock();
        let gone: Vec<PathBuf> = sessions.keys().filter(|p| p.starts_with(dir)).cloned().collect();
        for p in gone {
            if let Some(s) = sessions.remove(&p) {
                park(s);
            }
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        for (_, s) in self.sessions.lock().drain() {
            park(s);
        }
    }
}
