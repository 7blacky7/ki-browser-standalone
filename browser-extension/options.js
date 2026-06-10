const urlEl = document.getElementById('url');
const tokenEl = document.getElementById('token');
const savedEl = document.getElementById('saved');

async function load() {
  const cfg = await ext.storage.local.get(['kiBrowserUrl', 'kiBrowserToken']);
  urlEl.value = cfg.kiBrowserUrl || '';
  tokenEl.value = cfg.kiBrowserToken || '';
}

document.getElementById('save').addEventListener('click', async () => {
  const url = urlEl.value.trim();
  const token = tokenEl.value.trim();
  try {
    await ext.storage.local.set({ kiBrowserUrl: url, kiBrowserToken: token });
    // Read back to confirm it actually persisted (surfaces silent failures).
    const check = await ext.storage.local.get('kiBrowserUrl');
    if (check.kiBrowserUrl !== url) throw new Error('Storage nicht persistiert');
    savedEl.style.color = '#047857';
    savedEl.textContent = 'Gespeichert.';
  } catch (e) {
    savedEl.style.color = '#b91c1c';
    savedEl.textContent = 'Fehler: ' + e.message;
  }
  setTimeout(() => (savedEl.textContent = ''), 2500);
});

load();
