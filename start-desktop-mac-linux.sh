#!/usr/bin/env sh
# Opens the WEPANET desktop application (macOS / Linux).
# With Rust installed (https://rustup.rs) it is built, or brought up to date, first;
# the first build takes a few minutes. Without Rust the last build in desktop-app/ is used.
ROOT="$(cd "$(dirname "$0")" && pwd)"
EXE="$ROOT/desktop-app/wepanet"
if command -v cargo >/dev/null 2>&1; then
    echo "Building WEPANET Desktop..."
    cargo build --release -p epanet-ui --manifest-path "$ROOT/Cargo.toml" || exit 1
    TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
    mkdir -p "$ROOT/desktop-app"
    cp "$TARGET/release/epanet-ui" "$EXE"
fi
if [ ! -x "$EXE" ]; then
    echo "WEPANET Desktop has not been built yet. Install Rust from https://rustup.rs and run this file again."
    exit 1
fi
exec "$EXE" "$@"
