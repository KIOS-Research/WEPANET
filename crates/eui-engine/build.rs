//! Compiles the vendored OWA EPANET sources plus the EPANET-UI wrapper into a
//! static library. Skipped for wasm32, where the engine is the separately built
//! Emscripten module loaded by the web app.

use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=csrc");
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        return;
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/epanet");
    let mut build = cc::Build::new();
    for dir in ["src", "src/util"] {
        for entry in std::fs::read_dir(root.join(dir)).expect("vendor/epanet is missing") {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "c") {
                build.file(&path);
            }
        }
    }
    build
        .file("csrc/eui_engine.c")
        .include(root.join("include"))
        .include(root.join("src"))
        .define("DLLEXPORT", Some(""))
        .define("_CRT_SECURE_NO_WARNINGS", None)
        .warnings(false)
        .opt_level(2)
        .compile("epanet2");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        println!("cargo:rustc-link-lib=m");
    }
    println!("cargo:rerun-if-changed={}", root.display());
}
