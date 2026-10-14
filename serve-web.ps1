# Builds the web app and serves it at http://127.0.0.1:8080 (opens the browser).
# The page must be served over http: browsers block WebAssembly and Web Workers
# on file:// pages, so double-clicking index.html does not work.
#
# Usage (PowerShell, from the repository folder):   .\serve-web.ps1
# Requires: Rust + wasm32 target, trunk; Emscripten only if the engine is missing.

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
Remove-Item Env:NO_COLOR -ErrorAction SilentlyContinue  # trunk rejects NO_COLOR=1

# Keep build output out of Dropbox/OneDrive, which lock files while syncing.
if (-not $env:CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA "cargo-target\epanet-ui-web"
}

# Build the EPANET engine (Emscripten) once, if it is not there yet.
$engine = Join-Path $root "apps\web\engine\epanet_engine.wasm"
if (-not (Test-Path $engine)) {
    $emsdk = Join-Path $env:LOCALAPPDATA "emsdk"
    if (-not (Test-Path "$emsdk\upstream\emscripten\emcc.exe")) {
        throw "The EPANET WebAssembly engine is missing and Emscripten was not found in $emsdk. See README (Web app)."
    }
    $env:EMSDK = $emsdk
    $env:EM_CONFIG = "$emsdk\.emscripten"
    $env:PATH = "$emsdk\upstream\emscripten;$env:PATH"
    bash (Join-Path $root "web-engine/build.sh")
}

rustup target add wasm32-unknown-unknown | Out-Null
if (-not (Get-Command trunk -ErrorAction SilentlyContinue)) {
    cargo install trunk --locked
}

Set-Location (Join-Path $root "apps\web")
trunk serve --release --open --port 8080 --dist (Join-Path $env:LOCALAPPDATA "eui-web-serve")
