// Functions injected into the page via scripting.executeScript.
// They MUST be self-contained (no closure over extension scope) because they
// run in the page's world.

// Reads localStorage + sessionStorage of the current document.
function grabStorage() {
  const dump = (store) => {
    const out = {};
    try {
      for (let i = 0; i < store.length; i++) {
        const k = store.key(i);
        if (k !== null) out[k] = store.getItem(k);
      }
    } catch (e) {
      // storage may be blocked (e.g. third-party); return what we have
    }
    return out;
  };
  return {
    origin: location.origin,
    local: dump(window.localStorage),
    session: dump(window.sessionStorage)
  };
}

// Reads the fingerprint visible to the page (navigator/screen/WebGL/timezone).
// Field names match the ki-browser backend IdentitySpec (identity.rs).
function grabFingerprint() {
  let webglVendor = '';
  let webglRenderer = '';
  try {
    const canvas = document.createElement('canvas');
    const gl =
      canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
    if (gl) {
      const ext = gl.getExtension('WEBGL_debug_renderer_info');
      if (ext) {
        webglVendor = gl.getParameter(ext.UNMASKED_VENDOR_WEBGL) || '';
        webglRenderer = gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) || '';
      }
    }
  } catch (e) {
    // WebGL may be unavailable; leave strings empty
  }

  let timezone = '';
  try {
    timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || '';
  } catch (e) {}

  const languages =
    Array.isArray(navigator.languages) && navigator.languages.length
      ? Array.from(navigator.languages)
      : navigator.language
      ? [navigator.language]
      : [];

  // Only include numeric hardware hints when the real browser exposes a
  // non-zero value. The backend (identity.rs apply_overrides) rejects
  // hardware_concurrency==0 / device_memory==0 with a hard error, which would
  // abort the whole tab creation. Firefox does not expose navigator.deviceMemory.
  const fp = {
    user_agent: navigator.userAgent || '',
    platform: navigator.platform || '',
    languages: languages,
    webgl_vendor: webglVendor,
    webgl_renderer: webglRenderer,
    timezone: timezone
  };
  if (navigator.hardwareConcurrency > 0) {
    fp.hardware_concurrency = navigator.hardwareConcurrency;
  }
  if (navigator.deviceMemory > 0) {
    fp.device_memory = navigator.deviceMemory;
  }
  if (window.screen && window.screen.width > 0 && window.screen.height > 0) {
    fp.screen = { width: window.screen.width, height: window.screen.height };
  }
  return fp;
}
