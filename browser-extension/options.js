const urlEl = document.getElementById('url');
const tokenEl = document.getElementById('token');
const savedEl = document.getElementById('saved');

async function load() {
  const cfg = await ext.storage.local.get(['kiBrowserUrl', 'kiBrowserToken']);
  urlEl.value = cfg.kiBrowserUrl || '';
  tokenEl.value = cfg.kiBrowserToken || '';
}

document.getElementById('save').addEventListener('click', async () => {
  await ext.storage.local.set({
    kiBrowserUrl: urlEl.value.trim(),
    kiBrowserToken: tokenEl.value.trim()
  });
  savedEl.textContent = 'Gespeichert.';
  setTimeout(() => (savedEl.textContent = ''), 1500);
});

load();
