// WEPANET's Python for plugins in the browser.
// Exposes window.euiPython.run(files, requirements, onProgress) -> Promise<{response, network}>,
// used by the Rust app (crates/eui-ui/src/python.rs). Python (Pyodide) runs in a
// Web Worker, loaded on the first Python plugin; `files` maps file names to
// bytes (the runner, the plugin, the project, the request and the results),
// `requirements` lists the PyPI packages the plugin needs. onProgress(text)
// tells how it goes; the promise resolves to the runner's response.json text
// and the project the plugin left (or null).
(function () {
  const base = (document.currentScript && document.currentScript.src)
    ? document.currentScript.src.replace(/[^/]*$/, '')
    : 'engine/';
  let worker = null;
  let nextId = 1;
  const pending = new Map();

  function getWorker() {
    if (worker) return worker;
    worker = new Worker(base + 'python-worker.js', { type: 'module' });
    worker.onmessage = (e) => {
      const { id, result, error, progress } = e.data;
      const p = pending.get(id);
      if (!p) return;
      if (progress) {
        if (p.onProgress) p.onProgress(progress);
        return;
      }
      pending.delete(id);
      error ? p.reject(error) : p.resolve(result);
    };
    worker.onerror = (e) => {
      for (const p of pending.values()) p.reject('Python failed to load: ' + (e.message || e));
      pending.clear();
      worker = null;
    };
    return worker;
  }

  window.euiPython = {
    run(files, requirements, onProgress) {
      return new Promise((resolve, reject) => {
        const id = nextId++;
        pending.set(id, { onProgress, resolve, reject });
        getWorker().postMessage({ id, files, requirements: Array.from(requirements || []) });
      });
    },
  };
})();
