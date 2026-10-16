// WEPANET web engine bridge.
// Exposes window.euiEngine.run(inpText, msxText, onProgress) -> Promise<{code, report, out, energy, extra}>
// and window.euiEngine.cancel(), used by the Rust app (crates/eui-engine/src/web.rs).
// Simulations run in a Web Worker so the UI never freezes, using OWA EPANET
// compiled with Emscripten (web-engine/build.sh). onProgress(stage, hour) is
// called as the run goes (stage 1 = hydraulics, 2 = water quality, 3 = EPANET-MSX).
// msxText (or null) adds an EPANET-MSX analysis; its results come back as
// {msx, msxCode, msxReport, msxMessage}.
(function () {
  const base = (document.currentScript && document.currentScript.src)
    ? document.currentScript.src.replace(/[^/]*$/, '')
    : 'engine/';
  let worker = null;
  let nextId = 1;
  // Runs in the order the worker takes them: id -> {inp, onProgress, resolve, reject}.
  const pending = new Map();

  function getWorker() {
    if (worker) return worker;
    worker = new Worker(base + 'engine-worker.js');
    worker.onmessage = (e) => {
      const { id, result, error, progress } = e.data;
      const p = pending.get(id);
      if (!p) return;
      if (progress) {
        if (p.onProgress) p.onProgress(progress[0], progress[1]);
        return;
      }
      pending.delete(id);
      error ? p.reject(error) : p.resolve(result);
    };
    worker.onerror = (e) => {
      for (const p of pending.values()) p.reject('engine failed to load: ' + (e.message || e));
      pending.clear();
      worker = null;
    };
    return worker;
  }

  window.euiEngine = {
    run(inp, msx, onProgress) {
      return new Promise((resolve, reject) => {
        const id = nextId++;
        const msg = { id, inp, msx };
        pending.set(id, { msg, onProgress, resolve, reject });
        getWorker().postMessage(msg);
      });
    },
    // Converts units the way EPANET does (Project Setup); resolves to INP text.
    convert(inp, flow, press, hloss) {
      return new Promise((resolve, reject) => {
        const id = nextId++;
        const msg = { id, inp, convert: [flow, press, hloss] };
        pending.set(id, { msg, resolve, reject });
        getWorker().postMessage(msg);
      });
    },
    // Stops the run in progress: the worker is busy inside EPANET, so it is
    // terminated; runs queued behind it go to a new worker.
    cancel() {
      if (!worker || pending.size === 0) return;
      worker.terminate();
      worker = null;
      const [first, ...rest] = pending.entries();
      pending.delete(first[0]);
      first[1].reject('cancelled');
      for (const [, p] of rest) getWorker().postMessage(p.msg);
    },
  };
})();
