# ki-browser Session Grabber (WebExtension)

Grabs the **session** (cookies + localStorage + sessionStorage) and the
**fingerprint** (user agent, platform, languages, hardware, screen, WebGL
vendor/renderer, timezone) of the site you are logged into in your **real**
browser, and exports it as a *session bundle* that ki-browser can import.

This solves the login chicken-and-egg problem on anti-bot sites: you log in
normally in your everyday browser, grab the session here, and ki-browser opens a
tab that **inherits that session** and is **fingerprint-consistent** with the
browser you logged in from.

One codebase runs on **Chrome (MV3)** and **Firefox**.

## Bundle format

The produced JSON matches the ki-browser backend contract exactly:

```jsonc
{
  "version": 1,
  "created_at": "<ISO 8601>",
  "origin": "https://service.example.com",
  "cookies": [
    { "name", "value", "domain", "path", "secure", "httpOnly", "sameSite", "expires"? }
  ],
  "storage": [
    { "origin": "https://...", "local": { "k": "v" }, "session": { "k": "v" } }
  ],
  "fingerprint": {
    "user_agent", "platform", "languages": [],
    "hardware_concurrency", "device_memory",
    "screen": { "width", "height" },
    "webgl_vendor", "webgl_renderer", "timezone"
  }
}
```

The `fingerprint` field names are identical to the backend `IdentitySpec`
(`src/api/identity.rs`), so it maps directly onto `resolve_identity`.

## Install — Chrome / Edge

1. Open `chrome://extensions`.
2. Enable **Developer mode** (top right).
3. Click **Load unpacked** and select this `browser-extension/` folder.

## Install — Firefox

1. Open `about:debugging#/runtime/this-firefox`.
2. Click **Load Temporary Add-on…**.
3. Select the `manifest.json` inside this folder.

(Temporary add-ons are removed on Firefox restart; reload the same way.)

## Usage

1. Open and **log in** to the target site in your normal browser tab.
2. Open the extension popup and click **Session sichern**.
3. Export the bundle:
   - **Download JSON** — saves `<host>-session.json`.
   - **An ki-browser senden** — POSTs to `POST /session/import` on the configured
     ki-browser instance.
4. Configure the ki-browser URL (and optional Bearer token) under
   **Einstellungen** / the extension options page.

## Security note

The bundle contains **authentication cookies and storage** — it is equivalent to
being logged in. Treat the downloaded JSON like a password:

- Do not commit it or share it.
- Prefer sending it over **HTTPS** to a trusted ki-browser instance.
- ki-browser stores imported bundles **encrypted at rest**; cookie values are
  never logged in clear text.
