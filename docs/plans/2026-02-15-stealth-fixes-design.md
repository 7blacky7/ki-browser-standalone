# Stealth Bot-Detection Fixes

## Problem
CreepJS erkennt den Browser zu 33% als headless und 40% als stealth. 5 Schwachstellen identifiziert.

## Root Cause
`chromium_engine.rs` hat ein hardcoded Stealth-Script das `StealthConfig` (webrtc, canvas, audio) ignoriert.

## Tasks

### 1. chromium-stealth-integration
Hardcoded Script in chromium_engine.rs durch StealthConfig.get_complete_override_script() ersetzen.
Chrome-spezifische Extras (chrome.runtime, chrome.loadTimes) als separates Supplement behalten.

### 2. useragentdata-override
navigator.userAgentData Override in navigator.rs: brands, mobile, platform, getHighEntropyValues().
Chrome-Version aus UA-String extrahieren und konsistent in brands verwenden.

### 3. screen-viewport-sync
Screen-Resolution mit tatsaechlichem Viewport (1280x720) synchronisieren.
Orientation auf landscape-primary setzen. availWidth/availHeight korrekt berechnen.

### 4. webgl-comprehensive
WebGL Override erweitern: OffscreenCanvas, WEBGL_debug_renderer_info Extension, WebGPU adapter info.

### 5. stealth-test-verification
Nach allen Fixes: Rebuild, headless Test auf bot.sannysoft.com + CreepJS. Ergebnisse dokumentieren.
