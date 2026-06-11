# ki-browser smoke / resilience harness

`smoke.sh` drives a running ki-browser instance over its REST API and asserts the
**core capabilities** an AI agent needs in order to drive the browser. Every
check makes a **real assertion on response content** (not just HTTP 200), so a
green run means the feature actually works end-to-end. It is designed to be a
fast regression gate after a build/deploy.

## Requirements

- POSIX `sh`
- `curl`
- `jq`

## Usage

```sh
# default target http://192.168.50.65:9222
./smoke.sh

# explicit base URL (first positional argument)
./smoke.sh http://192.168.50.65:9333
```

Exit code:

- `0` — every check PASSed (`RESULT: ALL GREEN`)
- `1` — at least one check FAILed (or a dependency is missing). The summary
  lists the failed checks.

### Tunable timeouts (env vars)

| Var             | Default | Meaning                                                        |
|-----------------|---------|----------------------------------------------------------------|
| `CURL_TIMEOUT`  | `15`    | Per-request timeout (seconds) for normal calls (`curl -m`).    |
| `POPUP_TIMEOUT` | `5`     | Tight timeout for the popup-resilience responsiveness probes.  |

```sh
CURL_TIMEOUT=30 POPUP_TIMEOUT=5 ./smoke.sh http://host:9333
```

## Design notes

- **No nested shell quoting around `data:` URLs.** All JSON request bodies and
  the `data:text/html` documents are built with `jq -nc --arg` (and the HTML is
  URL-encoded with `jq @uri`). Hand-quoting HTML inside `data:` URLs was a real
  source of breakage; the harness avoids it entirely.
- **`/evaluate` uses the field name `script`** (not `code`/`expression`), and
  reads the result from `.data.result`.
- The standard response envelope is `{ success, data, error }`; assertions read
  values out of `.data`.
- `set -u` is enabled; robust `curl -m` timeouts guard against hangs; failed
  network calls surface as the sentinel `__CURL_ERR__` so a *hang* is
  distinguishable from a valid-but-error JSON response.
- All created tabs and sessions are tracked and cleaned up at the end, even on
  partial failure.

## What each check asserts

| # | Check | Assertion |
|---|-------|-----------|
| 1 | **/health** | `GET /health` responds and `status` matches `healthy`. |
| 2 | **Tab lifecycle** | `POST /tabs/new` (a `data:text/html` doc with `<title>SmokeLifecycle</title>` + `<h1 id="x">`) returns a `tab_id`; `GET /tabs` contains it; `/evaluate document.title` equals `SmokeLifecycle`; `POST /tabs/close` succeeds; the tab is then absent from `/tabs`. |
| 3 | **Click** | A `data:html` page with a `<button onclick="document.title=42">`. `POST /click {selector:"#btn"}` then `/evaluate` confirms `document.title == 42`. |
| 4 | **Type + Scroll** | `POST /type {selector:"#inp", text:"smoke-typed-123"}` then `/evaluate` asserts the input's `value`; `POST /scroll {delta_y:800}` completes without error. |
| 5 | **Multi-tab identity isolation** | 3 tabs created **in parallel** via `/tabs/new` each with an explicit, distinct `identity.timezone` (`Europe/Berlin`, `America/New_York`, `Asia/Tokyo`). `GET /tabs/{id}/identity` per tab asserts each reports **its own** timezone, and that the three reported zones are 3 distinct values (no cross-contamination between tabs). |
| 6 | **Popup resilience** (critical regression) | Open a tab, then `/evaluate window.open("https://example.com","_blank")`. Immediately probe with a tight `POPUP_TIMEOUT`: a `/tabs` call **and** a `/evaluate document.title` must each answer within the timeout. If either hangs (no answer in time) the check FAILs as a **popup hang**. This guards the 30s-hang regression. |
| 7 | **Session inheritance** | `POST /login-session/import` with a minimal bundle (origin `https://example.com`, 1 visible cookie `smoke_cookie`, 1 `localStorage` entry `smoke_ls`) returns a `session_id`; `GET /login-session/list` contains it; `POST /tabs/new {url:"https://example.com/", session_id}` creates a tab; inside that tab `document.cookie` and `localStorage` are asserted to contain the inherited values; `DELETE /login-session/{id}` removes it. |
| 8 | **Cleanup** | Closes every test tab and deletes every test session created during the run (best-effort, always runs). |

## Interpreting failures

- `popup hang: no answer within Ns` on check 6 — the browser stopped responding
  after a `window.open`. This is the key resilience regression; investigate the
  popup/new-window handling path.
- `expected [X] got [Y]` — the endpoint responded but with the wrong content
  (e.g. identity timezone crossed between tabs, or an inherited cookie missing).
- `no response within Ns` / `__CURL_ERR__` — the endpoint timed out or the
  instance is unreachable at `BASE_URL`.
