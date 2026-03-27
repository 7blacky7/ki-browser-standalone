# ki-browser-standalone

High-performance browser automation in Rust with REST API, stealth capabilities, and a full debug toolkit for AI agents.

Built on CEF (Chromium Embedded Framework) with CDP (Chrome DevTools Protocol) integration for privileged JS evaluation that bypasses CSP/Trusted Types.

## Features

- **REST API** — 50+ endpoints on port 3000 for complete browser control
- **CDP Client** — JS evaluation that bypasses CSP/Trusted Types (works on Gemini, LinkedIn, etc.)
- **Stealth Mode** — Chrome/Edge profiles, WebGL via SwiftShader, consistent HTTP+JS identity
- **Debug Toolkit** — 21 endpoints: Performance, CSS Inspector, Cookies, Network Interceptor, Console Capture
- **Consent Handler** — Automatic cookie consent for Sourcepoint, Cookiebot, OneTrust, TCF API
- **CAPTCHA Detection** — Detects reCAPTCHA, Cloudflare Turnstile, hCaptcha, Gameforge Image-Drop
- **Human-like Input** — Bezier curve mouse, Fitts' Law timing, realistic keyboard delays
- **WebSocket Streaming** — Live JPEG/H.264 frame streaming + real-time events
- **Multi-Agent** — Tab ownership, agent registration, batch operations, sessions
- **OCR** — Tesseract, PaddleOCR, Surya backends
- **Vision Labels** — Numbered overlays on interactive elements for AI analysis

## Quick Start

```bash
# Build
cargo build --release

# Start (headless + stealth + Full HD)
LD_LIBRARY_PATH="./target/release" ./target/release/ki-browser \
  --headless --port 3000 --stealth --max-tabs 10 --width 1920 --height 1080

# Health check
curl http://localhost:3000/health

# Create tab + navigate
TAB=$(curl -s -X POST localhost:3000/tabs/new \
  -H 'Content-Type: application/json' \
  -d '{"url":"https://example.com"}' | python3 -c "import json,sys; print(json.load(sys.stdin)['data']['tab_id'])")

# Evaluate JavaScript (works on CSP-protected sites via CDP)
curl -s -X POST localhost:3000/evaluate \
  -d "{\"tab_id\":\"$TAB\",\"script\":\"document.title\"}"

# Screenshot with zoom
curl -s "localhost:3000/screenshot?tab_id=$TAB&format=jpeg&quality=90&clip_x=0&clip_y=0&clip_width=800&clip_height=600&clip_scale=2"

# Auto-accept cookie consent
curl -s -X POST localhost:3000/debug/consent/accept -d "{\"tab_id\":\"$TAB\"}"

# Detect + solve CAPTCHAs
curl -s -X POST localhost:3000/debug/captcha/solve -d "{\"tab_id\":\"$TAB\"}"
```

## API Reference

### Core

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check |
| GET | `/tabs` | List all tabs |
| POST | `/tabs/new` | Create tab (`url?`, `active?`) |
| POST | `/tabs/close` | Close tab (`tab_id`) |
| POST | `/navigate` | Navigate (`tab_id`, `url`) |
| POST | `/click` | Click (`tab_id`, `selector?` or `x,y`) |
| POST | `/type` | Type text (`tab_id`, `text`, `selector?`) |
| POST | `/scroll` | Scroll (`tab_id`, `delta_y`) |
| POST | `/evaluate` | JS eval via CDP (`tab_id`, `script`, `frame_id?`) |
| POST | `/drag` | Drag & drop (`tab_id`, `from_x/y`, `to_x/y`) |
| GET | `/screenshot` | Screenshot (`tab_id`, `format?`, `clip_*?`, `full_page?`) |
| GET | `/frames` | iFrame tree |

### DOM & Vision

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/dom/element` | Find element by selector |
| POST | `/dom/annotate` | Screenshot with labeled elements |
| GET | `/dom/snapshot` | DOM tree as JSON |
| GET | `/vision/labels` | Vision labels JSON |
| POST | `/dom/extract-content` | Readability-like text extraction |
| POST | `/dom/extract-structured-data` | JSON-LD, OpenGraph, Meta |
| POST | `/dom/forms` | Detect forms |

### Debug Toolkit

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/debug/performance/timing` | Navigation Timing (DNS, TTFB, Load) |
| GET | `/debug/performance/vitals` | Web Vitals (LCP, FCP, CLS) |
| GET | `/debug/performance/memory` | JS Heap |
| POST | `/debug/css/computed` | Computed styles for element |
| POST | `/debug/css/box-model` | Box model (margin, padding, border) |
| GET | `/debug/cookies/:tab_id` | List cookies |
| POST | `/debug/cookies/:tab_id/set` | Set cookie |
| POST | `/debug/network/start` | Start network capture |
| GET | `/debug/network/entries` | Captured requests |
| POST | `/debug/console/start` | Start console capture |
| GET | `/debug/console` | Console logs |
| POST | `/debug/consent/accept` | Auto-accept cookie consent |
| POST | `/debug/captcha/detect` | Detect CAPTCHA type |
| POST | `/debug/captcha/solve` | Solve checkbox CAPTCHAs |
| GET | `/debug/popups` | Intercepted popup URLs |

### Sessions & Agents

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/session/register` | Register agent |
| POST | `/tabs/:tab_id/claim` | Claim tab ownership |
| POST | `/batch` | Batch operations |
| GET | `/ws` | WebSocket events |
| GET | `/ws/viewer` | Live frame stream |

## Stealth

Random Chrome/Edge profile per session. HTTP User-Agent and JS `navigator.userAgent` are identical (single identity).

| Feature | Status |
|---------|--------|
| `navigator.webdriver` | `false` |
| User Agent | Chrome 142-144 / Edge 143-144 |
| Platform | Win32 / MacIntel / Linux x86_64 |
| Plugins | 5 (Chrome PDF viewers) |
| WebGL | SwiftShader ANGLE (headless) or real GPU |
| Canvas Noise | Enabled |
| WebRTC Leak Prevention | Enabled |
| Automation Signal Removal | Selenium, CDP, PhantomJS markers removed |

**Tested against:** Google (15 searches, no CAPTCHA), Amazon, Booking, LinkedIn, bot.sannysoft.com (1 FAIL vs Playwright's 3 FAILs).

## Architecture

```
REST API (Axum 0.7)
  → IPC Channel → BrowserCommandHandler
    → CDP Client (WebSocket to port 9222, bypasses CSP)
    → CEF Engine (Chromium 144, single-process)
      → Stealth Injection (on_load_start)
      → Off-Screen Rendering (SwiftShader)
      → Input Simulation (Bezier, Fitts' Law)
```

### Key Modules

| Module | Files | Purpose |
|--------|-------|---------|
| `api/` | 49 | REST API, IPC, WebSocket, debug toolkit |
| `api/debug_routes/` | 10 | Performance, CSS, cookies, network, console, consent, CAPTCHA |
| `api/cdp_client.rs` | 1 | CDP WebSocket client for privileged JS eval |
| `browser/cef_engine/` | 9 | CEF integration, callbacks, input, navigation |
| `stealth/` | 19 | Fingerprint, navigator, WebGL, canvas, audio, WebRTC |
| `input/` | 5 | Mouse (Bezier), keyboard, timing (Fitts' Law) |
| `config/` | 2 | TOML/JSON/ENV/CLI configuration chain |

## Configuration

```bash
# CLI
./ki-browser --headless --stealth --port 3000 --max-tabs 10 --width 1920 --height 1080

# Environment
KI_BROWSER_API_PORT=3000 KI_BROWSER_STEALTH_MODE=true ./ki-browser

# Config file (config.toml)
window_width = 1920
window_height = 1080
headless = true
api_port = 3000
stealth_mode = true
max_tabs = 10
```

**Important:** `LD_LIBRARY_PATH` must include the CEF libraries directory (e.g. `./target/release`).

## Building

```bash
# Default (headless CEF)
cargo build --release

# With GUI (eframe/egui window)
cargo build --release --features gui

# With H.264 streaming
cargo build --release --features h264

# Run tests (490+ unit + integration tests)
cargo test
```

## License

MIT
