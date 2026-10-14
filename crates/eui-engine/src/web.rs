//! Browser bridge to the Emscripten build of EPANET (see `web-engine/`).
//!
//! `apps/web/engine/engine.js` defines `window.euiEngine.run(inpText, onProgress)`,
//! which runs the simulation in a Web Worker, calls `onProgress(stage, hour)` as
//! it goes and resolves to `{ code, report, out, energy }`, and
//! `window.euiEngine.cancel()`, which stops it.

use js_sys::{Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::{ENERGY_SIZE, EngineError, Progress, RunOutput};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = euiEngine, js_name = run)]
    fn js_run(
        inp: &str,
        on_progress: &Closure<dyn FnMut(i32, f64)>,
    ) -> Result<js_sys::Promise, JsValue>;
    #[wasm_bindgen(catch, js_namespace = euiEngine, js_name = cancel)]
    fn js_cancel() -> Result<(), JsValue>;
}

/// Stops the running simulation (its promise then rejects with "cancelled").
pub fn cancel() {
    let _ = js_cancel();
}

fn js_err(v: JsValue) -> EngineError {
    if v.as_string().as_deref() == Some("cancelled") {
        return EngineError::Cancelled;
    }
    EngineError::Unavailable(v.as_string().unwrap_or_else(|| format!("{v:?}")))
}

pub async fn run(
    inp: &str,
    progress: impl Fn(Progress) + 'static,
) -> Result<RunOutput, EngineError> {
    // kept alive until the run is over
    let on_progress = Closure::<dyn FnMut(i32, f64)>::new(move |stage, hour: f64| {
        progress(Progress::new(stage, hour as i64))
    });
    let promise = js_run(inp, &on_progress).map_err(js_err)?;
    let obj = JsFuture::from(promise).await.map_err(js_err)?;
    let get = |k: &str| Reflect::get(&obj, &JsValue::from_str(k)).map_err(js_err);
    let code = get("code")?.as_f64().unwrap_or(0.0) as i32;
    let report = get("report")?.as_string().unwrap_or_default();
    let out_val = get("out")?;
    let out = if out_val.is_instance_of::<Uint8Array>() {
        Uint8Array::from(out_val).to_vec()
    } else {
        Vec::new()
    };
    // Older engine builds do not return the energy sums.
    let energy = get("energy")
        .ok()
        .filter(|v| v.is_instance_of::<js_sys::Float64Array>())
        .map(|v| js_sys::Float64Array::from(v).to_vec())
        .and_then(|v| <[f64; ENERGY_SIZE]>::try_from(v).ok());
    let extra_val = get("extra")?;
    let extra = if extra_val.is_instance_of::<Uint8Array>() {
        Uint8Array::from(extra_val).to_vec()
    } else {
        Vec::new()
    };
    Ok(RunOutput {
        code,
        report,
        out,
        energy,
        extra,
    })
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = euiEngine, js_name = convert)]
    fn js_convert(inp: &str, flow: i32, press: i32, hloss: i32)
    -> Result<js_sys::Promise, JsValue>;
}

/// Converts units with EPANET in the worker; resolves to the new INP text.
pub async fn convert(inp: &str, c: crate::UnitChange) -> Result<String, EngineError> {
    let promise = js_convert(inp, c.flow_units, c.pressure_units, c.headloss).map_err(js_err)?;
    let v = JsFuture::from(promise).await.map_err(js_err)?;
    v.as_string()
        .ok_or_else(|| EngineError::Unavailable("no converted input".into()))
}
