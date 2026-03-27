//! CDP (Chrome DevTools Protocol) WebSocket client.
//!
//! Provides privileged JS evaluation via `Runtime.evaluate` that bypasses
//! CSP/Trusted Types restrictions, plus `Input.insertText` for contenteditable
//! elements and `Page.addScriptToEvaluateOnNewDocument` for stealth injection.
//!
//! This module connects to CEF's built-in remote debugging port (typically 9222)
//! and communicates via the JSON-RPC-based CDP protocol over WebSocket.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::{debug, error, warn};

// ============================================================================
// Types
// ============================================================================

/// CDP target info from /json/list endpoint.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CdpTarget {
    id: String,
    #[serde(rename = "type")]
    target_type: String,
    title: String,
    url: String,
    web_socket_debugger_url: Option<String>,
}

/// CDP JSON-RPC request.
#[derive(Debug, Serialize)]
struct CdpRequest {
    id: i64,
    method: String,
    params: serde_json::Value,
}

/// CDP JSON-RPC response.
#[derive(Debug, Deserialize)]
struct CdpResponse {
    id: i64,
    result: Option<serde_json::Value>,
    error: Option<CdpError>,
}

#[derive(Debug, Deserialize)]
struct CdpError {
    code: i64,
    message: String,
}

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

// ============================================================================
// CdpClient
// ============================================================================

/// WebSocket-based CDP client for privileged browser operations.
///
/// Connects to CEF's remote debugging port and provides:
/// - `evaluate()` — JS evaluation that bypasses CSP/Trusted Types
/// - `insert_text()` — Text input for contenteditable elements
/// - `add_init_script()` — Script injection before any page JS
pub struct CdpClient {
    port: u16,
    cmd_id: AtomicI64,
    /// Cache: target_id -> WebSocket connection
    connections: RwLock<HashMap<String, Arc<RwLock<WsStream>>>>,
}

impl CdpClient {
    /// Create a new CDP client targeting the given port.
    pub fn new(port: u16) -> Self {
        Self {
            port,
            cmd_id: AtomicI64::new(1),
            connections: RwLock::new(HashMap::new()),
        }
    }

    /// Get the next command ID.
    fn next_id(&self) -> i64 {
        self.cmd_id.fetch_add(1, Ordering::Relaxed)
    }

    // ========================================================================
    // Target Discovery
    // ========================================================================

    /// Find the CDP target (page) matching the given URL.
    /// Returns the WebSocket debugger URL for that target.
    pub async fn find_target_by_url(&self, url: &str) -> Result<String, String> {
        let list_url = format!("http://127.0.0.1:{}/json/list", self.port);
        let resp = reqwest::get(&list_url)
            .await
            .map_err(|e| format!("CDP list failed: {}", e))?;

        let targets: Vec<CdpTarget> = resp
            .json()
            .await
            .map_err(|e| format!("CDP parse failed: {}", e))?;

        // Match by URL (exact or prefix)
        let target = targets
            .iter()
            .filter(|t| t.target_type == "page")
            .find(|t| t.url == url || t.url.starts_with(url) || url.starts_with(&t.url))
            .or_else(|| {
                // Fallback: find any page target
                targets.iter().find(|t| t.target_type == "page")
            });

        match target {
            Some(t) => match &t.web_socket_debugger_url {
                Some(ws_url) => Ok(ws_url.clone()),
                None => {
                    // Construct WS URL from target ID
                    Ok(format!("ws://127.0.0.1:{}/devtools/page/{}", self.port, t.id))
                }
            },
            None => Err("No page target found".to_string()),
        }
    }

    /// Find CDP target by tab URL, trying the tab's current URL.
    pub async fn find_target_ws_url(&self, tab_url: &str) -> Result<String, String> {
        self.find_target_by_url(tab_url).await
    }

    // ========================================================================
    // WebSocket Connection Management
    // ========================================================================

    /// Get or create a WebSocket connection to the given target.
    async fn get_connection(&self, ws_url: &str) -> Result<Arc<RwLock<WsStream>>, String> {
        // Check cache
        {
            let conns = self.connections.read().await;
            if let Some(conn) = conns.get(ws_url) {
                return Ok(conn.clone());
            }
        }

        // Create new connection
        debug!("CDP: connecting to {}", ws_url);
        let (ws_stream, _) = connect_async(ws_url)
            .await
            .map_err(|e| format!("CDP WebSocket connect failed: {}", e))?;

        let conn = Arc::new(RwLock::new(ws_stream));
        let mut conns = self.connections.write().await;
        conns.insert(ws_url.to_string(), conn.clone());
        Ok(conn)
    }

    /// Send a CDP command and wait for the response.
    async fn send_command(
        &self,
        ws_url: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id();
        let request = CdpRequest {
            id,
            method: method.to_string(),
            params,
        };

        let msg = serde_json::to_string(&request)
            .map_err(|e| format!("CDP serialize failed: {}", e))?;

        let conn = self.get_connection(ws_url).await?;
        let mut ws = conn.write().await;

        // Send
        ws.send(Message::Text(msg.into()))
            .await
            .map_err(|e| {
                // Remove broken connection from cache
                error!("CDP send failed: {}", e);
                format!("CDP send failed: {}", e)
            })?;

        // Read responses until we get ours (skip events)
        let timeout = tokio::time::Duration::from_secs(15);
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err("CDP response timeout".to_string());
            }

            match tokio::time::timeout(remaining, ws.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(resp) = serde_json::from_str::<CdpResponse>(&text) {
                        if resp.id == id {
                            if let Some(err) = resp.error {
                                return Err(format!("CDP error {}: {}", err.code, err.message));
                            }
                            return Ok(resp.result.unwrap_or(serde_json::Value::Null));
                        }
                        // Not our response (event or different command) — skip
                    }
                    // Not a valid CDP response — skip (could be an event)
                }
                Ok(Some(Ok(_))) => {
                    // Binary or other message type — skip
                }
                Ok(Some(Err(e))) => {
                    // Drop broken connection
                    drop(ws);
                    self.connections.write().await.remove(ws_url);
                    return Err(format!("CDP WebSocket error: {}", e));
                }
                Ok(None) => {
                    drop(ws);
                    self.connections.write().await.remove(ws_url);
                    return Err("CDP WebSocket closed".to_string());
                }
                Err(_) => {
                    return Err("CDP response timeout".to_string());
                }
            }
        }
    }

    // ========================================================================
    // Public API
    // ========================================================================

    /// Evaluate JavaScript via CDP Runtime.evaluate (bypasses CSP/Trusted Types).
    ///
    /// Returns the result as a JSON string, or an error message.
    pub async fn evaluate(&self, ws_url: &str, expression: &str) -> Result<String, String> {
        let params = serde_json::json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true,
            "userGesture": true
        });

        let result = self.send_command(ws_url, "Runtime.evaluate", params).await?;

        // Parse CDP result format: { "result": { "type": "string", "value": "..." } }
        if let Some(exception) = result.get("exceptionDetails") {
            let text = exception
                .get("text")
                .and_then(|t| t.as_str())
                .unwrap_or("Unknown error");
            return Err(format!("JS exception: {}", text));
        }

        if let Some(val) = result.get("result").and_then(|r| r.get("value")) {
            match val {
                serde_json::Value::String(s) => Ok(s.clone()),
                serde_json::Value::Null => Ok("null".to_string()),
                other => Ok(other.to_string()),
            }
        } else if let Some(val) = result.get("result") {
            // For non-serializable results, return type info
            let type_str = val
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("unknown");
            if type_str == "undefined" {
                Ok("undefined".to_string())
            } else {
                Ok(val.to_string())
            }
        } else {
            Ok(result.to_string())
        }
    }

    /// Insert text at the current cursor position via CDP Input.insertText.
    /// Works with contenteditable elements where CEF key events may fail.
    pub async fn insert_text(&self, ws_url: &str, text: &str) -> Result<(), String> {
        let params = serde_json::json!({ "text": text });
        self.send_command(ws_url, "Input.insertText", params).await?;
        Ok(())
    }

    /// Add a script to evaluate on every new document before page JS runs.
    /// Used for stealth injection that survives navigation and bypasses CSP.
    pub async fn add_init_script(&self, ws_url: &str, source: &str) -> Result<String, String> {
        let params = serde_json::json!({ "source": source });
        let result = self
            .send_command(ws_url, "Page.addScriptToEvaluateOnNewDocument", params)
            .await?;

        Ok(result
            .get("identifier")
            .and_then(|i| i.as_str())
            .unwrap_or("")
            .to_string())
    }

    /// Focus an element by selector via CDP Runtime.evaluate, then insert text.
    /// Combines element focus + text insertion for contenteditable fields.
    pub async fn focus_and_type(
        &self,
        ws_url: &str,
        selector: &str,
        text: &str,
    ) -> Result<(), String> {
        // Focus the element
        let focus_script = format!(
            r#"(()=>{{var el=document.querySelector('{}');if(!el)return 'not_found';el.focus();return 'focused'}})()"#,
            selector.replace('\'', "\\'")
        );
        let focus_result = self.evaluate(ws_url, &focus_script).await?;
        if focus_result.contains("not_found") {
            return Err(format!("Element not found: {}", selector));
        }

        // Small delay for focus to take effect
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Insert text via CDP
        self.insert_text(ws_url, text).await
    }

    /// Close all cached WebSocket connections.
    pub async fn close_all(&self) {
        let mut conns = self.connections.write().await;
        for (url, conn) in conns.drain() {
            let mut ws = conn.write().await;
            let _ = ws.close(None).await;
            debug!("CDP: closed connection to {}", url);
        }
    }
}

impl std::fmt::Debug for CdpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CdpClient")
            .field("port", &self.port)
            .finish()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cdp_request_serialization() {
        let req = CdpRequest {
            id: 1,
            method: "Runtime.evaluate".to_string(),
            params: serde_json::json!({"expression": "1+1", "returnByValue": true}),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(json.contains("Runtime.evaluate"));
        assert!(json.contains("\"id\":1"));
    }

    #[test]
    fn test_cdp_response_deserialization() {
        let json = r#"{"id":1,"result":{"result":{"type":"number","value":2,"description":"2"}}}"#;
        let resp: CdpResponse = serde_json::from_str(json).expect("deserialize");
        assert_eq!(resp.id, 1);
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_cdp_error_response() {
        let json = r#"{"id":2,"error":{"code":-32000,"message":"Target closed"}}"#;
        let resp: CdpResponse = serde_json::from_str(json).expect("deserialize");
        assert_eq!(resp.id, 2);
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32000);
    }

    #[test]
    fn test_cdp_target_deserialization() {
        let json = r#"{"id":"ABC123","type":"page","title":"Test","url":"https://example.com","webSocketDebuggerUrl":"ws://127.0.0.1:9222/devtools/page/ABC123"}"#;
        let target: CdpTarget = serde_json::from_str(json).expect("deserialize");
        assert_eq!(target.id, "ABC123");
        assert_eq!(target.target_type, "page");
        assert!(target.web_socket_debugger_url.is_some());
    }

    #[test]
    fn test_cdp_client_creation() {
        let client = CdpClient::new(9222);
        assert_eq!(client.port, 9222);
        assert_eq!(client.next_id(), 1);
        assert_eq!(client.next_id(), 2);
    }
}
