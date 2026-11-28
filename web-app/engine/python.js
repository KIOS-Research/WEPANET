// WEPANET's Python for plugins and the Python Notebook in the browser.
// Exposes window.euiPython.run(files, requirements, onProgress) -> Promise<{response, network}>
// and window.euiPython.cell(files | null, source, reload, fresh, dpi, onProgress) -> Promise<json text>,
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
    // A cell of the Python Notebook, with the project's files when they
    // changed (null otherwise), `fresh` for the first cell of a new session,
    // `dpi` for its figures; resolves to the notebook's answer (JSON).
    cell(files, source, reload, fresh, dpi, onProgress) {
      return new Promise((resolve, reject) => {
        const id = nextId++;
        pending.set(id, { onProgress, resolve, reject });
        getWorker().postMessage({ id, cell: { files: files || null, source, reload, fresh, dpi } });
      });
    },
  };
})();
