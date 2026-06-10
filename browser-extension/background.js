// Service worker. Currently no persistent background logic is required:
// the popup performs the grab/export on demand. Kept as an explicit entry so
// the MV3 manifest is valid on both Chrome and Firefox.
//
// Listener kept for potential future message routing (e.g. context-menu grab).
try {
  const api = typeof browser !== 'undefined' ? browser : chrome;
  api.runtime.onInstalled.addListener(() => {
    // no-op: configuration lives in storage.local, edited via options.html
  });
} catch (e) {
  // ignore in restricted contexts
}
