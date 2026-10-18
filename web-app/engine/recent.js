// WEPANET's recent projects in the browser. Where the browser lets a page
// open files by handle (Chrome, Edge: File System Access API), the handle of
// each project opened or saved is kept in IndexedDB under its file name, with
// when it was last used — where the file is, not a copy of it — and the file
// is read again from there, the browser asking the user to allow it. Exposes
// window.euiRecent: supported(), list() -> Promise<[name, ...]> (latest
// first), pick() -> Promise<{name, bytes}>, remember(handle),
// open(name) -> Promise<{name, bytes} | null>, remove(name) and clear(),
// used by the Rust app (crates/eui-ui/src/recent.rs).
(function () {
  let db = null;

  function store() {
    if (db) return db;
    db = new Promise((resolve, reject) => {
      const req = indexedDB.open('wepanet-recent', 1);
      req.onupgradeneeded = () => req.result.createObjectStore('handles');
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => {
        db = null;
        reject(String(req.error));
      };
    });
    return db;
  }

  function run(mode, work) {
    return store().then((d) => new Promise((resolve, reject) => {
      const tx = d.transaction('handles', mode);
      const req = work(tx.objectStore('handles'));
      tx.oncomplete = () => resolve(req.result === undefined ? null : req.result);
      tx.onerror = () => reject(String(tx.error));
    }));
  }

  async function read(handle) {
    const file = await handle.getFile();
    return { name: handle.name, bytes: new Uint8Array(await file.arrayBuffer()) };
  }

  const remember = (handle) =>
    run('readwrite', (s) => s.put({ handle, time: Date.now() }, handle.name));

  window.euiRecent = {
    supported: () => 'showOpenFilePicker' in window,
    list: async () => {
      const d = await store();
      return new Promise((resolve, reject) => {
        const out = [];
        const req = d.transaction('handles', 'readonly').objectStore('handles').openCursor();
        req.onsuccess = () => {
          const c = req.result;
          if (!c) {
            out.sort((a, b) => b.time - a.time);
            resolve(out.map((r) => r.name));
            return;
          }
          out.push({ name: c.key, time: c.value.time || 0 });
          c.continue();
        };
        req.onerror = () => reject(String(req.error));
      });
    },
    pick: async () => {
      const [handle] = await window.showOpenFilePicker({
        types: [{
          description: 'EPANET input file or WEPANET project',
          accept: { 'text/plain': ['.inp', '.gz'], 'application/zip': ['.wepanet'] },
        }],
      });
      await remember(handle);
      return read(handle);
    },
    remember,
    open: async (name) => {
      const entry = await run('readonly', (s) => s.get(name));
      const handle = entry && (entry.handle || entry);
      if (!handle || !handle.getFile) return null;
      if ((await handle.queryPermission({ mode: 'read' })) !== 'granted'
        && (await handle.requestPermission({ mode: 'read' })) !== 'granted') {
        throw new Error('Reading ' + name + ' was not allowed');
      }
      const file = await read(handle);
      await remember(handle);
      return file;
    },
    remove: (name) => run('readwrite', (s) => s.delete(name)),
    clear: () => run('readwrite', (s) => s.clear()),
  };
})();
