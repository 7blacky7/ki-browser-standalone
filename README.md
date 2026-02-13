# ki-browser-standalone

A high-performance, standalone browser automation library written in Rust. Ki-Browser provides a powerful HTTP API for browser control with built-in stealth capabilities, human-like input simulation, and comprehensive fingerprint management.

## Features

- **REST API Server** - Full HTTP API for browser automation on port 9222
- **WebSocket Support** - Real-time browser events and bidirectional communication
- **Tab Management** - Create, close, navigate, and switch between multiple tabs
- **Human-like Input Simulation**
  - Mouse movements with Bezier curve paths
  - Realistic timing and micro-jitter
  - Keyboard input with natural delays
- **Stealth Mode**
  - Browser fingerprint spoofing
  - Navigator and WebGL property overrides
  - Bot detection evasion
- **Screenshot Capture** - Full page or element-specific screenshots
- **JavaScript Execution** - Evaluate scripts in page context
- **DOM Interaction** - Find elements, click, type, scroll
- **Proxy Support** - HTTP, HTTPS, and SOCKS5 proxies
- **Configurable** - TOML/JSON configuration files with environment variable overrides

## Installation

### Prerequisites

- Rust 1.70 or later
- Cargo package manager

### Building from Source

```bash
# Clone the repository
git clone https://github.com/your-org/ki-browser-standalone.git
cd ki-browser-standalone

# Build in release mode
cargo build --release

# The binary will be at target/release/ki-browser
```

### Running

```bash
# Run with default settings
./target/release/ki-browser

# Run with custom port
KI_BROWSER_API_PORT=8080 ./target/release/ki-browser

# Run with configuration file
./target/release/ki-browser --config config.toml
```

## Usage Examples

### Basic Navigation

```bash
# Navigate to a URL
curl -X POST http://localhost:9222/navigate \
  -H "Content-Type: application/json" \
  -d '{"url": "https://example.com"}'

# Take a screenshot
curl "http://localhost:9222/screenshot?format=png" \
  --output screenshot.png
```

### Tab Management

```bash
# List all tabs
curl http://localhost:9222/tabs

# Create a new tab
curl -X POST http://localhost:9222/tabs/new \
  -H "Content-Type: application/json" \
  -d '{"url": "https://example.com", "active": true}'

# Close a tab
curl -X POST http://localhost:9222/tabs/close \
  -H "Content-Type: application/json" \
  -d '{"tab_id": "tab-uuid-here"}'
```

### Mouse and Keyboard

```bash
# Click at coordinates
curl -X POST http://localhost:9222/click \
  -H "Content-Type: application/json" \
  -d '{"x": 500, "y": 300, "button": "left"}'

# Click on element by selector
curl -X POST http://localhost:9222/click \
  -H "Content-Type: application/json" \
  -d '{"selector": "#submit-button"}'

# Type text
curl -X POST http://localhost:9222/type \
  -H "Content-Type: application/json" \
  -d '{"text": "Hello World", "selector": "#input-field"}'

# Scroll the page
curl -X POST http://localhost:9222/scroll \
  -H "Content-Type: application/json" \
  -d '{"delta_y": 500, "behavior": "smooth"}'
```

### JavaScript Execution

```bash
# Execute JavaScript
curl -X POST http://localhost:9222/evaluate \
  -H "Content-Type: application/json" \
  -d '{"script": "document.title", "await_promise": false}'
```

### Find Elements

```bash
# Find an element
curl "http://localhost:9222/dom/element?selector=%23my-element"
```

## Configuration Options

Ki-Browser supports configuration through:
1. Configuration files (TOML or JSON)
2. Environment variables (prefixed with `KI_BROWSER_`)
3. Command-line arguments

### Configuration File (config.toml)

```toml
# Window settings
window_width = 1920
window_height = 1080
headless = false

# API server
api_enabled = true
api_port = 9222

# Stealth mode
stealth_mode = true

# Browser behavior
max_tabs = 10
default_timeout_ms = 30000

# Optional: Custom user agent
# user_agent = "Mozilla/5.0 ..."

# Optional: Profile persistence
# profile_path = "./profiles/default"

# Optional: Proxy configuration
# [proxy]
# host = "proxy.example.com"
# port = 8080
# proxy_type = "http"
# username = "user"
# password = "pass"
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `KI_BROWSER_WINDOW_WIDTH` | Browser window width | 1280 |
| `KI_BROWSER_WINDOW_HEIGHT` | Browser window height | 720 |
| `KI_BROWSER_HEADLESS` | Run in headless mode | false |
| `KI_BROWSER_API_ENABLED` | Enable HTTP API | true |
| `KI_BROWSER_API_PORT` | API server port | 9222 |
| `KI_BROWSER_STEALTH_MODE` | Enable stealth features | false |
| `KI_BROWSER_MAX_TABS` | Maximum concurrent tabs | 10 |
| `KI_BROWSER_DEFAULT_TIMEOUT_MS` | Default operation timeout | 30000 |
| `KI_BROWSER_USER_AGENT` | Custom user agent string | - |
| `KI_BROWSER_PROFILE_PATH` | Browser profile directory | - |
| `KI_BROWSER_PROXY_HOST` | Proxy server host | - |
| `KI_BROWSER_PROXY_PORT` | Proxy server port | 8080 |
| `KI_BROWSER_PROXY_TYPE` | Proxy type (http/https/socks5) | http |
| `KI_BROWSER_PROXY_USERNAME` | Proxy authentication username | - |
| `KI_BROWSER_PROXY_PASSWORD` | Proxy authentication password | - |

## API Documentation

### Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check and version info |
| GET | `/tabs` | List all open tabs |
| POST | `/tabs/new` | Create a new tab |
| POST | `/tabs/close` | Close a tab |
| POST | `/navigate` | Navigate to URL |
| POST | `/click` | Click at coordinates or element |
| POST | `/type` | Type text into element |
| POST | `/evaluate` | Execute JavaScript |
| GET | `/screenshot` | Capture screenshot |
| POST | `/scroll` | Scroll the page |
| GET | `/dom/element` | Find element by selector |
| GET | `/api/status` | Get API server status |
| POST | `/api/toggle` | Enable/disable API |

### WebSocket Events

Connect to `ws://localhost:9222/ws` for real-time events:

- `TabCreated` - New tab opened
- `TabClosed` - Tab closed
- `NavigationStarted` - Page navigation began
- `NavigationCompleted` - Page finished loading
- `PageError` - JavaScript error occurred

## Architecture

```
+-------------------+     +------------------+     +------------------+
|                   |     |                  |     |                  |
|   HTTP Client     +---->+   REST API       +---->+   Browser        |
|                   |     |   (Axum)         |     |   Engine         |
+-------------------+     +--------+---------+     +--------+---------+
                                   |                        |
                                   v                        v
+-------------------+     +--------+---------+     +--------+---------+
|                   |     |                  |     |                  |
|   WebSocket       +---->+   IPC Channel    +---->+   Tab Manager    |
|   Client          |     |                  |     |                  |
+-------------------+     +------------------+     +--------+---------+
                                                            |
                          +------------------+              |
                          |                  |              |
                          |   Stealth        |<-------------+
                          |   Module         |
                          |                  |
                          +--------+---------+
                                   |
                          +--------v---------+
                          |                  |
                          |   Input          |
                          |   Simulation     |
                          |                  |
                          +------------------+

Module Overview:
+------------------+--------------------------------------------------+
| Module           | Description                                      |
+------------------+--------------------------------------------------+
| api/server       | HTTP server setup and state management           |
| api/routes       | REST endpoint handlers                           |
| api/websocket    | WebSocket connection handling                    |
| api/ipc          | Inter-process communication for browser control  |
| browser/engine   | Core browser automation engine                   |
| browser/tab      | Tab lifecycle management                         |
| browser/dom      | DOM query and manipulation                       |
| browser/screenshot| Screenshot capture utilities                    |
| config/settings  | Configuration loading and validation             |
| stealth/fingerprint| Browser fingerprint generation                 |
| stealth/navigator| Navigator property spoofing                      |
| stealth/webgl    | WebGL fingerprint protection                     |
| input/mouse      | Human-like mouse simulation                      |
| input/keyboard   | Keyboard input simulation                        |
| input/bezier     | Bezier curve path generation                     |
| input/timing     | Realistic timing delays                          |
+------------------+--------------------------------------------------+
```

## Stealth Features

Ki-Browser includes comprehensive stealth capabilities to avoid bot detection:

### Fingerprint Profiles

- Windows Chrome/Firefox/Edge
- macOS Chrome/Safari/Firefox
- Linux Chrome/Firefox

### Spoofed Properties

- User agent string
- Platform and vendor
- Screen resolution
- Timezone
- Language preferences
- Plugin list
- Font enumeration
- Canvas fingerprinting defense
- WebGL renderer masking

### Human-like Behavior

- Bezier curve mouse movements
- Random micro-jitter
- Natural typing delays
- Realistic click timing

## Development

```bash
# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run

# Format code
cargo fmt

# Run linter
cargo clippy
```

## License

MIT License

Copyright (c) 2024

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
