// The only seam between the settings page and the Rust app (phase 13).
//
// The page was written for pywebview: `window.pywebview.api.<name>(...args)` returns a promise
// of the dict `settings_api.py` answered, and `pywebviewready` fires when that object exists.
// Here every call is `invoke('py_api', {name, args})` (src/commands/mod.rs checks the window and
// the 32-name allowlist), so index.html, app.js, setup.js and preview.js stay as they were.
//
// Also here: the three `clawd://` events the shell and the page exchange.
//   page -> shell  clawd://ready      first frame is on its way: the shell shows the window
//   shell -> page  clawd://show-tab   a tab name (update, agents, setup): window.clawdShowTab
//   shell -> page  clawd://busy-close the window was asked to close during a firmware install:
//                                     window.clawdBusyClose
// Loaded before the page's own scripts. No network, no eval.
(function () {
  'use strict';

  var tauri = window.__TAURI__;
  var invoke = tauri && tauri.core && tauri.core.invoke;
  var events = tauri && tauri.event;

  function toError(e) {
    return e instanceof Error ? e : new Error(String(e));
  }

  // Outside the app (the page opened in a browser to look at the layout) there is nothing to call:
  // answer like a failed command so the page renders its error text instead of throwing.
  function missing() {
    return Promise.resolve({ ok: false, code: 'error', error: 'Chưa có: trang này đang chạy ngoài Clawd.' });
  }

  function call(name, args) {
    if (!invoke) return missing();
    // undefined cannot cross the bridge; pywebview sent it as null as well.
    var list = args.map(function (a) { return a === undefined ? null : a; });
    return invoke('py_api', { name: name, args: list }).catch(function (e) { throw toError(e); });
  }

  var api = new Proxy({}, {
    get: function (_target, name) {
      // `await api` / Promise.resolve(api) look for `then`; there is no such command.
      if (typeof name !== 'string' || name === 'then') return undefined;
      return function () { return call(name, Array.prototype.slice.call(arguments)); };
    },
  });
  window.pywebview = { api: api };

  if (events) {
    events.listen('clawd://show-tab', function (e) {
      var p = e.payload;
      var tab = typeof p === 'string' ? p : p && p.tab;
      if (tab && window.clawdShowTab) window.clawdShowTab(tab);
    });
    events.listen('clawd://busy-close', function () {
      if (window.clawdBusyClose) window.clawdBusyClose();
    });
  }

  // After DOMContentLoaded (every script has run and registered its listener) and one frame:
  // tell the shell, then let the page start talking to the app (`load()` asks for state; that
  // and the usage preview come after the first paint, never before).
  function afterPaint() {
    var done = false;
    function go() {
      if (done) return;
      done = true;
      if (events) events.emit('clawd://ready');
      window.dispatchEvent(new Event('pywebviewready'));
    }
    requestAnimationFrame(go);
    // rAF stands still while the window is minimised or hidden: do not wait for it forever.
    setTimeout(go, 150);
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', afterPaint, { once: true });
  } else {
    afterPaint();
  }
})();
