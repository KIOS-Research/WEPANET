// Runs WEPANET's Python plugins with Pyodide (CPython in WebAssembly), off the
// main thread. Each run gets a fresh folder with the files WEPANET sends, and
// python_runner.py runs the plugin there (see crates/eui-ui/src/python.rs).
// A module worker: Pyodide 314's pyodide.js does not load with importScripts
const PYODIDE = 'https://cdn.jsdelivr.net/pyodide/v314.0.7/full/';

let pyodidePromise = null;
let current = null; // the id of the run, for progress lines
const installed = new Set();

function post(id, text) {
  self.postMessage({ id, progress: text });
}

// Lines the runner prints that start with "@@progress " are progress.
function onLine(line) {
  if (current !== null && line.startsWith('@@progress ')) post(current, line.slice(11));
}

function python(id) {
  if (!pyodidePromise) {
    post(id, 'Loading Python (Pyodide)…');
    pyodidePromise = import(PYODIDE + 'pyodide.mjs').then(async ({ loadPyodide }) => {
      const py = await loadPyodide({ indexURL: PYODIDE });
      py.setStdout({ batched: onLine });
      py.setStderr({ batched: () => {} });
      await py.loadPackage('micropip');
      return py;
    });
    pyodidePromise.catch(() => { pyodidePromise = null; });
  }
  return pyodidePromise;
}

// The packages of Pyodide's distribution the plugin imports (numpy, pandas,
// …), then its REQUIREMENTS from PyPI. EPyT's own dependencies beyond numpy
// and cffi (plots, Excel) are left out: they are large and plugins seldom
// need them, and a plugin that does imports them (or lists them).
async function install(py, id, source, requirements) {
  await py.loadPackagesFromImports(source, { messageCallback: () => {} });
  const micropip = py.pyimport('micropip');
  try {
    for (const req of requirements) {
      const key = req.trim().toLowerCase();
      if (!key || installed.has(key)) continue;
      post(id, 'Installing ' + req + '…');
      if (key === 'epyt') {
        await py.loadPackage(['numpy', 'cffi'], { messageCallback: () => {} });
        await micropip.install.callKwargs(req, { deps: false });
      } else {
        await micropip.install(req);
      }
      installed.add(key);
    }
  } finally {
    micropip.destroy();
  }
}

self.onmessage = async (e) => {
  const { id, files, requirements } = e.data;
  try {
    const py = await python(id);
    const decoder = new TextDecoder();
    await install(py, id, decoder.decode(files['plugin.py']), requirements);
    const FS = py.FS;
    const dir = '/home/pyodide/wepanet';
    try { py.runPython(`import shutil; shutil.rmtree(${JSON.stringify(dir)}, ignore_errors=True)`); } catch (_) {}
    FS.mkdirTree(dir);
    for (const [name, bytes] of Object.entries(files)) {
      const path = dir + '/' + name;
      FS.mkdirTree(path.slice(0, path.lastIndexOf('/')));
      FS.writeFile(path, bytes);
    }
    post(id, 'Running…');
    current = id;
    try {
      const scope = py.toPy({ __name__: '__main__' });
      py.runPython(`import sys; sys.argv = ['python_runner.py', ${JSON.stringify(dir)}]`);
      await py.runPythonAsync(decoder.decode(files['python_runner.py']), { globals: scope });
      scope.destroy();
    } finally {
      current = null;
      py.runPython("import os; os.chdir('/home/pyodide')");
    }
    const read = (f) => { try { return FS.readFile(dir + '/' + f, { encoding: 'utf8' }); } catch (_) { return null; } };
    const response = read('response.json');
    if (response === null) throw new Error('Python stopped without an answer');
    self.postMessage({ id, result: { response, network: read('network_out.inp') } });
  } catch (err) {
    self.postMessage({ id, error: String((err && err.message) || err) });
  }
};
