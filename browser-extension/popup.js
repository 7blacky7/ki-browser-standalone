// Popup logic: grab cookies + storage + fingerprint for the active tab's origin,
// build a bundle in the exact format expected by the ki-browser backend, and
// export it (download and/or POST /session/import).

const statusEl = document.getElementById('status');
const originEl = document.getElementById('origin');
const grabBtn = document.getElementById('grab');
const downloadBtn = document.getElementById('download');
const sendBtn = document.getElementById('send');

let currentBundle = null;

function setStatus(msg, cls) {
  statusEl.textContent = msg;
  statusEl.className = cls || '';
}

async function getActiveTab() {
  const tabs = await ext.tabs.query({ active: true, currentWindow: true });
  return tabs && tabs[0];
}

// chrome.cookies.getAll filters by domain (incl. subdomains) when given `domain`.
// We collect cookies for the registrable-ish host by querying both the exact
// host and its parent domain, then de-duplicate.
async function grabCookies(hostname) {
  const seen = new Set();
  const result = [];
  const addAll = (list) => {
    for (const c of list || []) {
      const key = `${c.domain}|${c.path}|${c.name}`;
      if (seen.has(key)) continue;
      seen.add(key);
      result.push({
        name: c.name,
        value: c.value,
        domain: c.domain,
        path: c.path,
        secure: !!c.secure,
        httpOnly: !!c.httpOnly,
        sameSite: normalizeSameSite(c.sameSite),
        ...(typeof c.expirationDate === 'number'
          ? { expires: Math.floor(c.expirationDate) }
          : {})
      });
    }
  };

  // Exact host.
  addAll(await ext.cookies.getAll({ domain: hostname }));

  // Parent domain (covers subdomain cookies like .example.com).
  const parts = hostname.split('.');
  if (parts.length > 2) {
    const parent = parts.slice(-2).join('.');
    addAll(await ext.cookies.getAll({ domain: parent }));
  }
  return result;
}

function normalizeSameSite(s) {
  // chrome: 'no_restriction'|'lax'|'strict'|'unspecified'
  // bundle: 'None'|'Lax'|'Strict'
  switch ((s || '').toLowerCase()) {
    case 'strict': return 'Strict';
    case 'lax': return 'Lax';
    case 'no_restriction': return 'None';
    default: return 'Lax';
  }
}

async function injectGrab(tabId, func) {
  const res = await ext.scripting.executeScript({
    target: { tabId },
    func
  });
  return res && res[0] && res[0].result;
}

async function buildBundle() {
  const tab = await getActiveTab();
  if (!tab || !tab.url) throw new Error('Kein aktiver Tab.');
  const url = new URL(tab.url);
  if (!/^https?:$/.test(url.protocol)) {
    throw new Error('Aktive Seite ist keine http(s)-Seite.');
  }
  const origin = url.origin;

  const cookies = await grabCookies(url.hostname);
  const storage = await injectGrab(tab.id, grabStorage);
  const fingerprint = await injectGrab(tab.id, grabFingerprint);

  return {
    version: 1,
    created_at: new Date().toISOString(),
    origin,
    cookies,
    storage: storage ? [storage] : [],
    fingerprint
  };
}

function downloadBundle(bundle) {
  const host = (() => {
    try { return new URL(bundle.origin).hostname; } catch { return 'session'; }
  })();
  const blob = new Blob([JSON.stringify(bundle, null, 2)], {
    type: 'application/json'
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `${host}-session.json`;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 2000);
}

async function sendBundle(bundle) {
  const cfg = await ext.storage.local.get(['kiBrowserUrl', 'kiBrowserToken']);
  const base = (cfg.kiBrowserUrl || '').replace(/\/+$/, '');
  if (!base) throw new Error('Keine ki-browser-URL konfiguriert (Einstellungen).');
  const headers = { 'Content-Type': 'application/json' };
  if (cfg.kiBrowserToken) headers['Authorization'] = `Bearer ${cfg.kiBrowserToken}`;

  const resp = await fetch(`${base}/session/import`, {
    method: 'POST',
    headers,
    body: JSON.stringify({ bundle })
  });
  const text = await resp.text();
  if (!resp.ok) {
    throw new Error(`Server ${resp.status}: ${text.slice(0, 200)}`);
  }
  let sessionId = '';
  try { sessionId = JSON.parse(text).session_id || ''; } catch {}
  return sessionId;
}

grabBtn.addEventListener('click', async () => {
  setStatus('Sichere Session…');
  grabBtn.disabled = true;
  try {
    currentBundle = await buildBundle();
    downloadBtn.disabled = false;
    sendBtn.disabled = false;
    setStatus(
      `OK: ${currentBundle.cookies.length} Cookies, ` +
      `${(currentBundle.storage[0]?.local && Object.keys(currentBundle.storage[0].local).length) || 0} localStorage-Keys gesichert.`,
      'ok'
    );
  } catch (e) {
    setStatus(`Fehler: ${e.message}`, 'err');
  } finally {
    grabBtn.disabled = false;
  }
});

downloadBtn.addEventListener('click', () => {
  if (currentBundle) downloadBundle(currentBundle);
});

sendBtn.addEventListener('click', async () => {
  if (!currentBundle) return;
  setStatus('Sende an ki-browser…');
  sendBtn.disabled = true;
  try {
    const id = await sendBundle(currentBundle);
    setStatus(id ? `Gesendet. session_id=${id}` : 'Gesendet.', 'ok');
  } catch (e) {
    setStatus(`Fehler: ${e.message}`, 'err');
  } finally {
    sendBtn.disabled = false;
  }
});

document.getElementById('open-options').addEventListener('click', (e) => {
  e.preventDefault();
  if (ext.runtime.openOptionsPage) ext.runtime.openOptionsPage();
});

// Show active origin on open.
(async () => {
  try {
    const tab = await getActiveTab();
    originEl.textContent = tab && tab.url ? new URL(tab.url).origin : '(unbekannt)';
  } catch {
    originEl.textContent = '(unbekannt)';
  }
})();
