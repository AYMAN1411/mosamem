//! The models end to end, on real photos. Ignored by default (it needs the model files and
//! photos); run with tools/ai/measure.sh, which sets:
//! - LAWHA_MODEL_DIR: a folder with the pack files (served locally, so the real install path runs:
//!   download → SHA-256 check → GPU rewrite);
//! - LAWHA_TEST_PHOTOS: a folder of PNGs;
//! - LAWHA_AI_REPORT: where to write the measurements (JSON).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use super::models::{self, PACKS};
use super::nets::{self, Click};
use super::runtime::Runtime;

fn serve(dir: PathBuf) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", server.server_addr().to_ip().unwrap());
    std::thread::spawn(move || {
        for rq in server.incoming_requests() {
            let path = dir.join(rq.url().trim_start_matches('/'));
            match std::fs::File::open(path) {
                Ok(f) => rq.respond(tiny_http::Response::from_file(f)).unwrap(),
                Err(_) => rq.respond(tiny_http::Response::empty(404)).unwrap(),
            }
        }
    });
    addr
}

fn iou(a: &[f32], b: &[f32]) -> f64 {
    let (mut i, mut u) = (0u64, 0u64);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (*x > 0.5, *y > 0.5);
        i += (x && y) as u64;
        u += (x || y) as u64;
    }
    i as f64 / u.max(1) as f64
}

/// PSNR (dB) of `got` against `want` over the hole's pixels (RGB).
fn psnr(want: &[u8], got: &[u8], hole: &[bool]) -> f64 {
    let (mut se, mut n) = (0f64, 0u64);
    for (i, h) in hole.iter().enumerate() {
        if *h {
            for c in 0..3 {
                let d = want[i * 3 + c] as f64 - got[i * 3 + c] as f64;
                se += d * d;
                n += 1;
            }
        }
    }
    10.0 * (255f64 * 255.0 / (se / n.max(1) as f64).max(1e-9)).log10()
}

fn ms(t: Instant) -> f64 {
    (t.elapsed().as_secs_f64() * 1000.0 * 10.0).round() / 10.0
}

#[test]
#[ignore]
fn models_on_photos() {
    let (Ok(model_dir), Ok(photos)) = (std::env::var("LAWHA_MODEL_DIR"), std::env::var("LAWHA_TEST_PHOTOS")) else {
        panic!("set LAWHA_MODEL_DIR and LAWHA_TEST_PHOTOS");
    };
    let root = std::env::temp_dir().join(format!("lawha-ai-it-{}", std::process::id()));
    let addr = serve(PathBuf::from(model_dir));
    let mut install_ms = serde_json::Map::new();
    for p in PACKS {
        let t = Instant::now();
        models::download_from(&root, p, &|f| vec![format!("{addr}/{}", f.name)], &AtomicBool::new(false), &|_| {}).unwrap();
        assert!(models::installed(&root, p));
        install_ms.insert(p.id.into(), ms(t).into());
    }
    let file = |pack: &str, name: &str| models::pack_dir(&root, models::pack(pack).unwrap()).join(name);
    let (subject, encoder, decoder, fill) = (
        file("subject", "birefnet_lite_fp16.onnx"),
        file("select", "vision_encoder_fp16.onnx"),
        file("select", "prompt_encoder_mask_decoder.onnx"),
        file("fill", "migan_pipeline_v2.onnx"),
    );
    let gpu = Runtime::default();
    let cpu = Runtime::cpu();
    let mut names: Vec<PathBuf> = std::fs::read_dir(&photos).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "png")).collect();
    names.sort();
    let mut rows = vec![];
    for (k, p) in names.iter().enumerate() {
        let img = image::open(p).unwrap().to_rgba8();
        let (w, h) = img.dimensions();
        let rgba = img.into_raw();
        let name = p.file_stem().unwrap().to_string_lossy().to_string();

        // subject: GPU, and the CPU for agreement (only on a few photos: the CPU is slow)
        let t = Instant::now();
        let s_gpu = nets::subject(&gpu, &subject, &rgba, w, h).unwrap();
        let subject_ms = ms(t);
        let cpu_iou = (k < 6).then(|| iou(&s_gpu, &nets::subject(&cpu, &subject, &rgba, w, h).unwrap()));

        // object selection: one click in the middle, compared with the subject matte
        let t = Instant::now();
        let e = nets::sam_encode(&gpu, &encoder, &rgba, w, h).unwrap();
        let encode_ms = ms(t);
        let t = Instant::now();
        let o = nets::sam_decode(&gpu, &decoder, &e, &[Click { x: w as f64 / 2.0, y: h as f64 / 2.0, add: true }], None).unwrap();
        let decode_ms = ms(t);
        let t = Instant::now();
        // a box drawn around the subject (its matte's bounds, 2 px out)
        let (mut bx0, mut by0, mut bx1, mut by1) = (w as f64, h as f64, 0f64, 0f64);
        for (i, v) in s_gpu.iter().enumerate() {
            if *v > 0.5 {
                let (x, y) = ((i % w as usize) as f64, (i / w as usize) as f64);
                (bx0, by0, bx1, by1) = (bx0.min(x), by0.min(y), bx1.max(x + 1.0), by1.max(y + 1.0));
            }
        }
        let bbox = [(bx0 - 2.0).max(0.0), (by0 - 2.0).max(0.0), (bx1 + 2.0).min(w as f64), (by1 + 2.0).min(h as f64)];
        let ob = nets::sam_decode(&gpu, &decoder, &e, &[], Some(bbox)).unwrap();
        let box_ms = ms(t);

        // fill: cut a square from the background corner, fill it, compare with what was there
        let side = (w.min(h) / 5) as usize;
        let (hx, hy) = ((w as usize) / 12, (h as usize) / 12);
        let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        let hole: Vec<bool> = (0..(w * h) as usize).map(|i| {
            let (x, y) = (i % w as usize, i / w as usize);
            x >= hx && x < hx + side && y >= hy && y < hy + side
        }).collect();
        let known: Vec<bool> = hole.iter().map(|b| !b).collect();
        let mut cut = rgb.clone();
        for (i, hh) in hole.iter().enumerate() {
            if *hh {
                cut[i * 3..i * 3 + 3].copy_from_slice(&[255, 0, 255]);
            }
        }
        let t = Instant::now();
        let ai = nets::fill(&gpu, &fill, &cut, &known, w, h).unwrap();
        let fill_ms = ms(t);
        let t = Instant::now();
        let f: Vec<[f32; 3]> = cut.chunks_exact(3).map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
        let classic: Vec<u8> = lawha_core::inpaint::inpaint(w as usize, h as usize, &f, &hole, 1).into_iter().flat_map(|p| p.map(|v| v.round().clamp(0.0, 255.0) as u8)).collect();
        let classic_ms = ms(t);
        // the simplest baseline: the hole's border color
        let mean: [f64; 3] = {
            let mut s = [0f64; 3];
            let mut n = 0f64;
            for (i, hh) in hole.iter().enumerate() {
                let (x, y) = (i % w as usize, i / w as usize);
                if !*hh && x + 2 >= hx && x < hx + side + 2 && y + 2 >= hy && y < hy + side + 2 {
                    for c in 0..3 {
                        s[c] += rgb[i * 3 + c] as f64;
                    }
                    n += 1.0;
                }
            }
            s.map(|v| v / n.max(1.0))
        };
        let flat: Vec<u8> = (0..rgb.len()).map(|i| if hole[i / 3] { mean[i % 3].round() as u8 } else { rgb[i] }).collect();

        rows.push(serde_json::json!({
            "photo": name, "size": [w, h],
            "subjectMs": subject_ms, "subjectGpuVsCpuIoU": cpu_iou,
            "samEncodeMs": encode_ms, "samClickMs": decode_ms, "samBoxMs": box_ms,
            "samClickVsSubjectIoU": iou(&o, &s_gpu), "samBoxVsSubjectIoU": iou(&ob, &s_gpu),
            "fillMs": fill_ms, "fillPsnr": psnr(&rgb, &ai, &hole),
            "classicMs": classic_ms, "classicPsnr": psnr(&rgb, &classic, &hole),
            "flatPsnr": psnr(&rgb, &flat, &hole),
        }));
        if k < 6 {
            if let Ok(dir) = std::env::var("LAWHA_AI_SAMPLES") {
                let save_mask = |v: &[f32], suffix: &str| {
                    image::GrayImage::from_raw(w, h, v.iter().map(|p| (p * 255.0) as u8).collect()).unwrap().save(format!("{dir}/{name}-{suffix}.png")).unwrap();
                };
                save_mask(&s_gpu, "subject");
                save_mask(&o, "click");
                save_mask(&ob, "box");
                image::RgbImage::from_raw(w, h, ai.clone()).unwrap().save(format!("{dir}/{name}-fill.png")).unwrap();
                image::RgbImage::from_raw(w, h, classic.clone()).unwrap().save(format!("{dir}/{name}-classic.png")).unwrap();
            }
        }
        eprintln!("{}", rows.last().unwrap());
    }
    let devices: serde_json::Map<String, serde_json::Value> = [("subject", &subject), ("samEncoder", &encoder), ("samDecoder", &decoder), ("fill", &fill)]
        .iter()
        .map(|(k, p)| (k.to_string(), serde_json::json!(if gpu.on_gpu(p) == Some(true) { "webgpu" } else { "cpu" })))
        .collect();
    let report = serde_json::json!({ "installMs": install_ms, "devices": devices, "photos": rows });
    if let Ok(out) = std::env::var("LAWHA_AI_REPORT") {
        std::fs::write(out, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// The control for tools/ai/no-network.sh: inside its sandbox a real download must fail.
#[test]
#[ignore]
fn upstream_download_is_blocked() {
    let root = std::env::temp_dir().join(format!("lawha-ai-blocked-{}", std::process::id()));
    let p = models::pack("fill").unwrap();
    let r = models::download(&root, p, &AtomicBool::new(false), &|_| {});
    let _ = std::fs::remove_dir_all(&root);
    assert!(matches!(r, Err(models::DownloadError::Network(_))), "the download was not blocked: {r:?}");
}

/// Object Selection's post-processing on a real photo: the click's matte, then the edge snap.
#[test]
#[ignore]
fn object_selection_keeps_its_edges() {
    let dir = PathBuf::from(std::env::var("LAWHA_MODEL_DIR").expect("LAWHA_MODEL_DIR"));
    let photo = std::env::var("LAWHA_TEST_PHOTO").expect("LAWHA_TEST_PHOTO");
    let img = image::open(photo).unwrap().to_rgba8();
    let (w, h) = img.dimensions();
    let rgba = img.into_raw();
    let rt = Runtime::default();
    let e = nets::sam_encode(&rt, &dir.join("vision_encoder_fp16.onnx"), &rgba, w, h).unwrap();
    let p = nets::sam_decode(&rt, &dir.join("prompt_encoder_mask_decoder.onnx"), &e, &[Click { x: w as f64 * 0.45, y: h as f64 * 0.45, add: true }], None).unwrap();
    let hist = |d: &[u8]| { let mut c = [0usize; 4]; for v in d { c[(*v >> 6) as usize] += 1; } c };
    let dense = super::to_dense(w, h, &p);
    let snapped = super::snap_edges(&dense, &rgba, 256);
    eprintln!("matte {:?} snapped {:?}", hist(&dense.data), hist(&snapped.data));
    let inside = snapped.data.iter().filter(|v| **v >= 192).count();
    let outside = snapped.data.iter().filter(|v| **v < 64).count();
    assert!(inside > 1000 && outside > 1000, "a clear inside and outside");
}
