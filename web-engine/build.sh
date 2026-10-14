#!/usr/bin/env bash
# Builds OWA EPANET + the EPANET-UI wrapper to WebAssembly with Emscripten.
# Output: apps/web/engine/epanet_engine.{js,wasm}
#
# Needs emcc on PATH (https://emscripten.org), or run through Docker:
#   docker run --rm -v "$PWD:/src" -w /src emscripten/emsdk bash web-engine/build.sh
set -euo pipefail
cd "$(dirname "$0")/.."
EP=vendor/epanet
emcc -O3 \
  "$EP"/src/*.c "$EP"/src/util/*.c crates/eui-engine/csrc/eui_engine.c \
  -I"$EP"/include -I"$EP"/src -DDLLEXPORT= -w \
  -sMODULARIZE=1 -sEXPORT_NAME=createEpanet -sENVIRONMENT=worker \
  -sALLOW_MEMORY_GROWTH=1 -sFORCE_FILESYSTEM=1 \
  -sEXPORTED_FUNCTIONS='["_eui_run","_eui_run_energy","_eui_run_progress","_eui_convert_units","_eui_version","_malloc","_free"]' \
  -sEXPORTED_RUNTIME_METHODS='["FS","ccall","HEAPF64"]' \
  -o apps/web/engine/epanet_engine.js
ls -la apps/web/engine/epanet_engine.*
