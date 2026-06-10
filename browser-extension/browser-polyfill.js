// Minimal cross-browser namespace shim.
// Firefox exposes `browser` (promise-based). Chrome exposes `chrome` (callback-based
// for older APIs, but MV3 cookies/storage/scripting return promises when no callback
// is passed). We expose a single `ext` object that works on both.
//
// For the small surface this extension uses (cookies.getAll, storage.local,
// scripting.executeScript, tabs.query) both engines provide promise-returning
// variants, so a thin alias is enough.
(function (global) {
  const api = typeof browser !== 'undefined' ? browser : chrome;
  global.ext = api;
})(typeof self !== 'undefined' ? self : this);
