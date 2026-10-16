// Runs WEPANET's Python plugins and its Python Console with Pyodide (CPython
// in WebAssembly), off the main thread. Each plugin run gets a fresh folder
// with the files WEPANET sends, and python_runner.py runs the plugin there; the
// console keeps one session of python_runner.py, fed line by line (see
// crates/eui-ui/src/python.rs). Messages are handled one at a time.
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

function writeFiles(FS, dir, files) {
  FS.mkdirTree(dir);
  for (const [name, bytes] of Object.entries(files)) {
    const path = dir + '/' + name;
    FS.mkdirTree(path.slice(0, path.lastIndexOf('/')));
    FS.writeFile(path, bytes);
  }
}

// The console's session: python_runner.py loaded once, as a module.
let consoleScope = null;

// A line typed in the console; resolves to console_push's JSON answer.
async function consoleLine(id, files, line, reload) {
  const py = await python(id);
  const dir = '/home/pyodide/wepanet_console';
  if (files) writeFiles(py.FS, dir, files);
  if (!consoleScope) {
    if (!files || !files['python_runner.py']) throw new Error('the console has no runner');
    consoleScope = py.toPy({ __name__: 'wepanet_runner' });
    py.runPython(new TextDecoder().decode(files['python_runner.py']), { globals: consoleScope });
  }
  const answer = (output) => JSON.stringify({ output, more: false, response: null, network: null });
  // pip install <packages>, outside a block: micropip
  const words = line.trim().replace(/^%/, '').split(/\s+/);
  const session = consoleScope.get('_console');
  const inBlock = session && session.buffer.length > 0;
  if (words[0] === 'pip' && words[1] === 'install' && words.length > 2 && !inBlock) {
    try {
      await install(py, id, '', words.slice(2));
      return answer('Installed ' + words.slice(2).join(', ') + '\n');
    } catch (err) {
      return answer(String((err && err.message) || err) + '\n');
    }
  }
  // the packages of Pyodide's distribution the line imports
  try { await py.loadPackagesFromImports(line, { messageCallback: () => {} }); } catch (_) {}
  current = id;
  try {
    return consoleScope.get('console_push')(dir, line, reload);
  } finally {
    current = null;
  }
}

async function handle(data) {
  const { id, files, requirements } = data;
  if (data.console) {
    try {
      const { files: f, line, reload } = data.console;
      self.postMessage({ id, result: await consoleLine(id, f, line, reload) });
    } catch (err) {
      self.postMessage({ id, error: String((err && err.message) || err) });
    }
    return;
  }
  try {
    const py = await python(id);
    const decoder = new TextDecoder();
    await install(py, id, decoder.decode(files['plugin.py']), requirements);
    const FS = py.FS;
    const dir = '/home/pyodide/wepanet';
    try { py.runPython(`import shutil; shutil.rmtree(${JSON.stringify(dir)}, ignore_errors=True)`); } catch (_) {}
    writeFiles(FS, dir, files);
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
}

let queue = Promise.resolve();
self.onmessage = (e) => {
  queue = queue.then(() => handle(e.data));
};
