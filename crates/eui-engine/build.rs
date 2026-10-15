//! Compiles the vendored OWA EPANET and EPANET-MSX sources plus the EPANET-UI
//! wrapper into static libraries. Skipped for wasm32, where the engine is the
//! separately built Emscripten module loaded by the web app.

use std::path::{Path, PathBuf};

/// The C files of a vendored source folder.
fn sources(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap_or_else(|_| panic!("{} is missing", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "c"))
        .collect()
}

fn main() {
    println!("cargo:rerun-if-changed=csrc");
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        return;
    }

    let vendor = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor");
    let root = vendor.join("epanet");
    let msx = vendor.join("epanet-msx");
    let common = |build: &mut cc::Build| {
        build
            .include(root.join("include"))
            .define("DLLEXPORT", Some(""))
            .define("MSXDLLEXPORT", Some(""))
            .define("_CRT_SECURE_NO_WARNINGS", None)
            .warnings(false)
            .opt_level(2);
    };

    // EPANET-MSX, with the one global whose name EPANET also uses renamed.
    let mut build = cc::Build::new();
    common(&mut build);
    build
        .files(sources(&msx.join("src")))
        .include(msx.join("include"))
        .define("Q_STAGNANT", Some("MSX_Q_STAGNANT"))
        .compile("epanetmsx");

    let mut build = cc::Build::new();
    common(&mut build);
    build
        .files(sources(&root.join("src")))
        .files(sources(&root.join("src/util")))
        .file("csrc/eui_engine.c")
        .include(root.join("src"))
        .include(msx.join("include"))
        .compile("epanet2");
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => {}
        // EPANET-MSX can load compiled reaction functions with dlopen.
        Ok("linux") => {
            println!("cargo:rustc-link-lib=m");
            println!("cargo:rustc-link-lib=dl");
        }
        _ => println!("cargo:rustc-link-lib=m"),
    }
    println!("cargo:rerun-if-changed={}", root.display());
    println!("cargo:rerun-if-changed={}", msx.display());
}
