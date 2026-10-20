fn main() {
    // ONNX Runtime's WebGPU provider links libwebgpu_dawn.dylib, which sits next to the executable
    // in development and in the app bundle's Frameworks folder once packaged.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
        // ONNX Runtime's prebuilt objects check the macOS version at run time (`@available`), which
        // needs clang's runtime library; rustc doesn't link it by default.
        if let Ok(out) = std::process::Command::new("xcrun").args(["clang", "--print-runtime-dir"]).output() {
            let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if std::path::Path::new(&dir).join("libclang_rt.osx.a").exists() {
                println!("cargo:rustc-link-search=native={dir}");
                println!("cargo:rustc-link-lib=static=clang_rt.osx");
            }
        }
    }
    // macOS: the app bundle carries libwebgpu_dawn.dylib (bundle.macOS.frameworks). ONNX Runtime's
    // build script (ort-sys, which runs before this one) has put it in the build folder; copy it
    // where the config points, so a fresh checkout builds in any profile.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
        if let Some(profile) = out.ancestors().nth(3) {
            let dylib = profile.join("libwebgpu_dawn.dylib");
            if dylib.exists() {
                std::fs::create_dir_all("frameworks").unwrap();
                std::fs::copy(std::fs::canonicalize(&dylib).unwrap(), "frameworks/libwebgpu_dawn.dylib").expect("copy libwebgpu_dawn.dylib");
            }
        }
    }
    // Windows: the installer carries DirectML.dll (bundle.resources in tauri.windows.conf.json).
    // ONNX Runtime's build script (ort-sys, which runs before this one) has put it in the build
    // folder; copy it where the bundle config points, before tauri-build checks that it exists.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
        // OUT_DIR is <target>/<triple>/<profile>/build/<pkg>/out
        if let Some(profile) = out.ancestors().nth(3) {
            let dll = profile.join("DirectML.dll");
            let dest = std::path::Path::new("windows/DirectML.dll");
            if dll.exists() {
                std::fs::copy(std::fs::canonicalize(&dll).unwrap(), dest).expect("copy DirectML.dll");
            }
        }
    }
    tauri_build::build()
}
