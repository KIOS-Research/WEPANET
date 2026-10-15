// Runs EPANET (Emscripten build, epanet_engine.js) off the main thread.
importScripts('epanet_engine.js');

let modulePromise = null;
function engine() {
  if (!modulePromise) modulePromise = createEpanet();
  return modulePromise;
}

self.onmessage = async (e) => {
  const { id, inp } = e.data;
  try {
    const M = await engine();
    const dir = '/run' + id;
    M.FS.mkdir(dir);
    M.FS.chdir(dir); // EPANET writes its scratch files into the working directory
    // Encoded here: Emscripten turns a string into a plain JS Array, one
    // element per byte, which fails ("Invalid array length") above ~134 MB.
    M.FS.writeFile('net.inp', new TextEncoder().encode(inp));
    const read = (f, opts) => { try { return M.FS.readFile(f, opts); } catch (_) { return null; } };
    const cleanup = () => {
      for (const f of M.FS.readdir('.')) if (f !== '.' && f !== '..') M.FS.unlink(f);
      M.FS.chdir('/');
      M.FS.rmdir(dir);
    };
    // Unit conversion (Project Setup): eui_convert_units in eui_engine.c.
    if (e.data.convert) {
      const [flow, press, hloss] = e.data.convert;
      const code = M.ccall('eui_convert_units', 'number',
        ['string', 'string', 'string', 'number', 'number', 'number'],
        ['net.inp', 'net.rpt', 'new.inp', flow, press, hloss]);
      const text = code >= 100 ? null : read('new.inp', { encoding: 'utf8' });
      cleanup();
      self.postMessage(text === null
        ? { id, error: 'EPANET error ' + code }
        : { id, result: text });
      return;
    }
    // Progress (stage, hour) from inside the engine (eui_js_progress in eui_engine.c).
    self.euiProgress = (stage, hour) => self.postMessage({ id, progress: [stage, hour] });
    // Energy balance sums (9 doubles) added up at every hydraulic time step.
    const ENERGY_SIZE = 9;
    const ptr = M._malloc(8 * ENERGY_SIZE);
    let code, energy;
    try {
      code = M.ccall('eui_run_progress', 'number',
        ['string', 'string', 'string', 'string', 'number', 'number', 'number'],
        ['net.inp', 'net.rpt', 'net.out', 'net.ext', ptr, 0, 0]);
      energy = code >= 100 ? null : new Float64Array(M.HEAPF64.buffer, ptr, ENERGY_SIZE).slice();
    } finally {
      M._free(ptr);
    }
    const report = read('net.rpt', { encoding: 'utf8' }) || '';
    const out = code >= 100 ? new Uint8Array(0) : (read('net.out') || new Uint8Array(0));
    const extra = code >= 100 ? new Uint8Array(0) : (read('net.ext') || new Uint8Array(0));
    cleanup();
    self.postMessage({ id, result: { code, report, out, energy, extra } }, [out.buffer, extra.buffer]);
  } catch (err) {
    self.postMessage({ id, error: String(err && err.message || err) });
  }
};
