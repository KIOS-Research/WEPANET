// Local Python launcher bridge. Network bytes stay inside the browser.
(() => {
  let pending = null;
  let sender = null;
  let announced = false;
  let received = false;
  window.addEventListener('message', (event) => {
    if (window.parent === window || event.source !== window.parent || received) return;
    let origin;
    try { origin = new URL(event.origin); } catch { return; }
    if (origin.protocol !== 'http:' || !['127.0.0.1', 'localhost', '[::1]'].includes(origin.hostname)) return;
    const data = event.data;
    if (!data || data.type !== 'wepanet:open-inp' || typeof data.name !== 'string' ||
        !/\.inp(?:\.gz)?$/i.test(data.name) || !(data.bytes instanceof ArrayBuffer) ||
        data.bytes.byteLength > 64 * 1024 * 1024) return;
    received = true;
    pending = [data.name, new Uint8Array(data.bytes)];
    sender = [event.source, event.origin];
    // Wake egui even when the browser is idle.
    const canvas = document.getElementById('epanet_ui_canvas');
    if (canvas) canvas.dispatchEvent(new MouseEvent('mousemove', { bubbles: true }));
  });
  window.euiTakeLaunchFile = () => {
    if (!announced && window.parent !== window) {
      announced = true;
      window.parent.postMessage({ type: 'wepanet:ready' }, '*');
    }
    const file = pending;
    pending = null;
    if (file && sender) sender[0].postMessage({ type: 'wepanet:received' }, sender[1]);
    return file;
  };
})();
