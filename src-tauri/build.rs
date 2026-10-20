fn main() {
    // ONNX Runtime's WebGPU provider links libwebgpu_dawn.dylib, which sits next to the executable
    // in development and in the app bundle's Frameworks folder once packaged.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/../Frameworks");
    }
    tauri_build::build()
}
