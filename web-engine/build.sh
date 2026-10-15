#!/usr/bin/env bash
# Builds OWA EPANET + EPANET-MSX + the EPANET-UI wrapper to WebAssembly with
# Emscripten. Output: apps/web/engine/epanet_engine.{js,wasm}
#
# Needs emcc on PATH (https://emscripten.org), or run through Docker:
#   docker run --rm -v "$PWD:/src" -w /src emscripten/emsdk bash web-engine/build.sh
set -euo pipefail
cd "$(dirname "$0")/.."
EP=vendor/epanet
MSX=vendor/epanet-msx
OBJ=$(mktemp -d)
trap 'rm -rf "$OBJ"' EXIT

# EPANET-MSX, with the one global whose name EPANET also uses renamed
# (as crates/eui-engine/build.rs does).
for f in "$MSX"/src/*.c; do
  emcc -O3 -c "$f" -I"$EP"/include -I"$MSX"/include -DDLLEXPORT= -DMSXDLLEXPORT= \
    -DQ_STAGNANT=MSX_Q_STAGNANT -w -o "$OBJ/msx_$(basename "$f" .c).o"
done

emcc -O3 \
  "$EP"/src/*.c "$EP"/src/util/*.c crates/eui-engine/csrc/eui_engine.c "$OBJ"/*.o \
  -I"$EP"/include -I"$EP"/src -I"$MSX"/include -DDLLEXPORT= -DMSXDLLEXPORT= -w \
  -sMODULARIZE=1 -sEXPORT_NAME=createEpanet -sENVIRONMENT=worker \
  -sALLOW_MEMORY_GROWTH=1 -sFORCE_FILESYSTEM=1 \
  -sEXPORTED_FUNCTIONS='["_eui_run","_eui_run_energy","_eui_run_progress","_eui_run_msx","_eui_msx_error_message","_eui_convert_units","_eui_version","_malloc","_free"]' \
  -sEXPORTED_RUNTIME_METHODS='["FS","ccall","HEAPF64","UTF8ToString"]' \
  -o apps/web/engine/epanet_engine.js
ls -la apps/web/engine/epanet_engine.*
