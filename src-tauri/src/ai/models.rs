//! The AI models: which files make up each feature's model, where they come from, and how they get
//! onto the disk. Nothing is downloaded until the user asks for a feature that needs it, and every
//! file is checked against its SHA-256 before it is used. After that, everything runs offline.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use sha2::{Digest, Sha256};

pub struct ModelFile {
    /// Name on disk (models with external weights expect their data file next to them by name).
    pub name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
    /// Split wide Split/Concat nodes after download, so the model runs on the GPU path.
    pub rewrite_wide: bool,
}

pub struct Pack {
    pub id: &'static str,
    pub model: &'static str,
    pub license: &'static str,
    pub source: &'static str,
    pub files: &'static [ModelFile],
}

/// Licenses checked on 2026-10-20 (docs/research.md §10). URLs are pinned to a commit.
pub const PACKS: &[Pack] = &[
    Pack {
        id: "subject",
        model: "BiRefNet lite",
        license: "MIT",
        source: "https://huggingface.co/onnx-community/BiRefNet_lite-ONNX",
        files: &[ModelFile {
            name: "birefnet_lite_fp16.onnx",
            url: "https://huggingface.co/onnx-community/BiRefNet_lite-ONNX/resolve/de15b22ba131738a16dff04aab8bdf8dc32e3ac1/onnx/model_fp16.onnx",
            sha256: "d39b897ceb16ae654c1731f3dba0cf9b368d9cae74b5a57459b455cc8bfec402",
            size: 114_538_221,
            rewrite_wide: true,
        }],
    },
    Pack {
        id: "select",
        model: "SAM 2.1 tiny",
        license: "Apache-2.0",
        source: "https://huggingface.co/onnx-community/sam2.1-hiera-tiny-ONNX",
        files: &[
            ModelFile { name: "vision_encoder_fp16.onnx", url: "https://huggingface.co/onnx-community/sam2.1-hiera-tiny-ONNX/resolve/814a066640debee5a91e70aa401fb8e17e030503/onnx/vision_encoder_fp16.onnx", sha256: "7773e79d589dada8e1e630cc7eb18b17b2c5b4faeb1eaab5ebf9df618ecbd50a", size: 314_925, rewrite_wide: false },
            ModelFile { name: "vision_encoder_fp16.onnx_data", url: "https://huggingface.co/onnx-community/sam2.1-hiera-tiny-ONNX/resolve/814a066640debee5a91e70aa401fb8e17e030503/onnx/vision_encoder_fp16.onnx_data", sha256: "a4dd3759e9b6a476d991fb3493787992e78777e51462695e1bede71ae258103e", size: 67_005_504, rewrite_wide: false },
            ModelFile { name: "prompt_encoder_mask_decoder.onnx", url: "https://huggingface.co/onnx-community/sam2.1-hiera-tiny-ONNX/resolve/814a066640debee5a91e70aa401fb8e17e030503/onnx/prompt_encoder_mask_decoder.onnx", sha256: "874414704c5d686db7d206a35f6e15d26563d50c8c4468fccc6739bd7e491dcf", size: 213_114, rewrite_wide: false },
            ModelFile { name: "prompt_encoder_mask_decoder.onnx_data", url: "https://huggingface.co/onnx-community/sam2.1-hiera-tiny-ONNX/resolve/814a066640debee5a91e70aa401fb8e17e030503/onnx/prompt_encoder_mask_decoder.onnx_data", sha256: "e9874d900dd4134ed60eab1e97910327c2419e0b2954485d8fd6e7f1a1470f47", size: 20_958_208, rewrite_wide: false },
        ],
    },
    Pack {
        id: "fill",
        model: "MI-GAN",
        license: "MIT (code and weights)",
        source: "https://github.com/Picsart-AI-Research/MI-GAN",
        files: &[ModelFile {
            name: "migan_pipeline_v2.onnx",
            url: "https://huggingface.co/andraniksargsyan/migan/resolve/406830d0fa60666da0071c342ad2fbc8f30c5c64/migan_pipeline_v2.onnx",
            sha256: "6f1f3530a1a2324b19752018ce756088b07973cda8d7d890034ace5c8a48c40b",
            size: 28_079_181,
            rewrite_wide: false,
        }],
    },
];

pub fn pack(id: &str) -> Option<&'static Pack> {
    PACKS.iter().find(|p| p.id == id)
}

/// Where a file is downloaded from: the pinned upstream URL, or only `LAWHA_MODEL_MIRROR` when that
/// is set (a local folder served over HTTP, for offline installs and tests).
pub fn urls(f: &ModelFile) -> Vec<String> {
    match std::env::var("LAWHA_MODEL_MIRROR") {
        Ok(m) => vec![format!("{}/{}", m.trim_end_matches('/'), f.name)],
        Err(_) => vec![f.url.to_string()],
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PackStatus {
    pub id: &'static str,
    pub model: &'static str,
    pub license: &'static str,
    pub source: &'static str,
    /// Download size in bytes.
    pub size: u64,
    pub installed: bool,
    /// Size on disk once installed.
    pub disk: u64,
    /// Whether its model ran on the GPU (None until it has run).
    pub gpu: Option<bool>,
}

const STAMP: &str = "installed.json";

pub fn pack_dir(root: &Path, p: &Pack) -> PathBuf {
    root.join(p.id)
}

pub fn installed(root: &Path, p: &Pack) -> bool {
    let d = pack_dir(root, p);
    d.join(STAMP).is_file() && p.files.iter().all(|f| d.join(f.name).is_file())
}

pub fn status(root: &Path, rt: &super::runtime::Runtime) -> Vec<PackStatus> {
    PACKS
        .iter()
        .map(|p| {
            let d = pack_dir(root, p);
            let ok = installed(root, p);
            let disk = if ok { p.files.iter().filter_map(|f| fs::metadata(d.join(f.name)).ok()).map(|m| m.len()).sum() } else { 0 };
            PackStatus { id: p.id, model: p.model, license: p.license, source: p.source, size: p.files.iter().map(|f| f.size).sum(), installed: ok, disk, gpu: rt.on_gpu(&d.join(p.files[0].name)) }
        })
        .collect()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}

#[derive(Debug)]
pub enum DownloadError {
    Cancelled,
    Network(String),
    Checksum(&'static str),
    Disk(String),
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            DownloadError::Cancelled => write!(f, "cancelled"),
            DownloadError::Network(e) => write!(f, "network: {e}"),
            DownloadError::Checksum(n) => write!(f, "checksum: {n}"),
            DownloadError::Disk(e) => write!(f, "disk: {e}"),
        }
    }
}

fn disk(e: impl std::fmt::Display) -> DownloadError {
    DownloadError::Disk(e.to_string())
}

/// Downloads one file to `to`, hashing as it goes. Returns its SHA-256 (hex).
fn fetch(url: &str, to: &Path, done: &mut u64, total: u64, cancel: &AtomicBool, progress: &dyn Fn(Progress)) -> Result<String, DownloadError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(std::time::Duration::from_secs(20)))
        .timeout_recv_body(None)
        .http_status_as_error(true)
        .build()
        .into();
    let mut res = agent.get(url).call().map_err(|e| DownloadError::Network(e.to_string()))?;
    let mut reader = res.body_mut().with_config().limit(u64::MAX).reader();
    let mut file = fs::File::create(to).map_err(disk)?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 1 << 18];
    let mut last = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(DownloadError::Cancelled);
        }
        let n = reader.read(&mut buf).map_err(|e| DownloadError::Network(e.to_string()))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(disk)?;
        *done += n as u64;
        if *done - last >= 1 << 20 {
            last = *done;
            progress(Progress { done: *done, total });
        }
    }
    file.sync_all().map_err(disk)?;
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Downloads, checks and installs a pack. Files land in a temporary folder first, so a cancelled
/// or failed download never leaves a half-installed model behind.
pub fn download(root: &Path, p: &Pack, cancel: &AtomicBool, progress: &dyn Fn(Progress)) -> Result<(), DownloadError> {
    download_from(root, p, &urls, cancel, progress)
}

pub(crate) fn download_from(root: &Path, p: &Pack, sources: &dyn Fn(&ModelFile) -> Vec<String>, cancel: &AtomicBool, progress: &dyn Fn(Progress)) -> Result<(), DownloadError> {
    let dir = pack_dir(root, p);
    let tmp = root.join(format!(".{}.part", p.id));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(disk)?;
    let result = (|| {
        let total: u64 = p.files.iter().map(|f| f.size).sum();
        let mut done = 0u64;
        progress(Progress { done, total });
        for f in p.files {
            let to = tmp.join(f.name);
            let before = done;
            let mut last_err = None;
            let mut ok = false;
            for url in sources(f) {
                done = before;
                match fetch(&url, &to, &mut done, total, cancel, progress) {
                    Ok(sum) if sum == f.sha256 => {
                        ok = true;
                        break;
                    }
                    Ok(_) => last_err = Some(DownloadError::Checksum(f.name)),
                    Err(DownloadError::Cancelled) => return Err(DownloadError::Cancelled),
                    Err(e) => last_err = Some(e),
                }
            }
            if !ok {
                return Err(last_err.unwrap_or(DownloadError::Network("no source".into())));
            }
            if f.rewrite_wide {
                let bytes = fs::read(&to).map_err(disk)?;
                let (out, _) = super::onnx_wide::rewrite(&bytes).map_err(disk)?;
                if super::onnx_wide::widest_node(&out).map_err(disk)? > super::onnx_wide::LIMIT + 2 {
                    return Err(DownloadError::Disk(format!("{}: still has nodes too wide for the GPU", f.name)));
                }
                fs::write(&to, out).map_err(disk)?;
            }
        }
        progress(Progress { done: total, total });
        let stamp: Vec<_> = p.files.iter().map(|f| (f.name, f.sha256)).collect();
        fs::write(tmp.join(STAMP), serde_json::to_vec(&stamp).unwrap()).map_err(disk)?;
        let _ = fs::remove_dir_all(&dir);
        fs::rename(&tmp, &dir).map_err(disk)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&tmp);
    }
    result
}

pub fn remove(root: &Path, p: &Pack) -> std::io::Result<()> {
    match fs::remove_dir_all(pack_dir(root, p)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        r => r,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn serve(dir: PathBuf) -> (String, std::thread::JoinHandle<()>) {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = format!("http://{}", server.server_addr().to_ip().unwrap());
        let h = std::thread::spawn(move || {
            for _ in 0..4 {
                let Ok(Some(rq)) = server.recv_timeout(std::time::Duration::from_secs(5)) else { return };
                let path = dir.join(rq.url().trim_start_matches('/'));
                match fs::read(path) {
                    Ok(b) => rq.respond(tiny_http::Response::from_data(b)).unwrap(),
                    Err(_) => rq.respond(tiny_http::Response::empty(404)).unwrap(),
                }
            }
        });
        (addr, h)
    }

    #[test]
    fn every_file_is_pinned_and_hashed() {
        for p in PACKS {
            for f in p.files {
                assert_eq!(f.sha256.len(), 64, "{}", f.name);
                assert!(f.url.starts_with("https://huggingface.co/"), "{}", f.url);
                // a 40-hex commit, never a moving branch
                assert!(f.url.split('/').any(|s| s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())), "{}", f.url);
            }
        }
    }

    fn local(addr: String) -> impl Fn(&ModelFile) -> Vec<String> {
        move |f| vec![format!("{addr}/{}", f.name)]
    }

    #[test]
    fn checksum_mismatch_installs_nothing() {
        let root = std::env::temp_dir().join(format!("lawha-models-bad-{}", std::process::id()));
        let served = root.join("served");
        fs::create_dir_all(&served).unwrap();
        fs::write(served.join("migan_pipeline_v2.onnx"), b"not the model").unwrap();
        let (addr, _h) = serve(served);
        let p = pack("fill").unwrap();
        let r = download_from(&root, p, &local(addr), &AtomicBool::new(false), &|_| {});
        assert!(matches!(r, Err(DownloadError::Checksum(_))), "{r:?}");
        assert!(!installed(&root, p));
        assert!(!root.join(".fill.part").exists(), "no partial files left");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unreachable_source_is_a_network_error() {
        let root = std::env::temp_dir().join(format!("lawha-models-off-{}", std::process::id()));
        let p = pack("fill").unwrap();
        let r = download_from(&root, p, &|f| vec![format!("http://127.0.0.1:9/{}", f.name)], &AtomicBool::new(false), &|_| {});
        assert!(matches!(r, Err(DownloadError::Network(_))), "{r:?}");
        assert!(!installed(&root, p));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_verified_download_installs_and_removes() {
        // a pack whose one file is served locally, with its real hash
        let root = std::env::temp_dir().join(format!("lawha-models-ok-{}", std::process::id()));
        let served = root.join("served");
        fs::create_dir_all(&served).unwrap();
        let body = b"pretend model".repeat(1000);
        fs::write(served.join("m.onnx"), &body).unwrap();
        let sum: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();
        let sum: &'static str = Box::leak(sum.into_boxed_str());
        let files: &'static [ModelFile] = Box::leak(Box::new([ModelFile { name: "m.onnx", url: "https://example.invalid/m.onnx", sha256: sum, size: body.len() as u64, rewrite_wide: false }]));
        let p = Pack { id: "t", model: "t", license: "MIT", source: "", files };
        let (addr, _h) = serve(served);
        let seen = std::sync::Mutex::new(vec![]);
        download_from(&root, &p, &local(addr), &AtomicBool::new(false), &|pr| seen.lock().unwrap().push(pr.done)).unwrap();
        assert!(installed(&root, &p));
        assert_eq!(*seen.lock().unwrap().last().unwrap(), body.len() as u64);
        remove(&root, &p).unwrap();
        assert!(!installed(&root, &p));
        let _ = fs::remove_dir_all(&root);
    }
}
