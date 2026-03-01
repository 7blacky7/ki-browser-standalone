//! Internal IPC for communication between API and browser core
//!
//! Provides a command/response pattern using tokio mpsc channels
//! for communicating browser control commands.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, error, warn};

/// Command ID counter for correlation
static NEXT_COMMAND_ID: AtomicU64 = AtomicU64::new(1);

/// IPC command message: (command_id, command, response_sender)
type IpcCommandMessage = (u64, IpcCommand, oneshot::Sender<IpcResponse>);

/// IPC commands for browser control
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcCommand {
    /// Create a new tab
    CreateTab {
        url: String,
        active: bool,
    },

    /// Close a tab
    CloseTab {
        tab_id: String,
    },

    /// Navigate to URL
    Navigate {
        tab_id: String,
        url: String,
    },

    /// Go back in history
    GoBack {
        tab_id: String,
    },

    /// Go forward in history
    GoForward {
        tab_id: String,
    },

    /// Reload the page
    Reload {
        tab_id: String,
        ignore_cache: bool,
    },

    /// Stop loading
    Stop {
        tab_id: String,
    },

    /// Click at coordinates
    ClickCoordinates {
        tab_id: String,
        x: i32,
        y: i32,
        button: String,
        modifiers: Option<Vec<String>>,
    },

    /// Drag from one position to another
    Drag {
        tab_id: String,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        steps: Option<u32>,
        duration_ms: Option<u64>,
    },

    /// Click on element by selector
    ClickElement {
        tab_id: String,
        selector: String,
        button: String,
        modifiers: Option<Vec<String>>,
        #[serde(default)]
        frame_id: Option<String>,
    },

    /// Type text
    TypeText {
        tab_id: String,
        text: String,
        selector: Option<String>,
        clear_first: bool,
        #[serde(default)]
        frame_id: Option<String>,
    },

    /// Press key
    PressKey {
        tab_id: String,
        key: String,
        modifiers: Option<Vec<String>>,
    },

    /// Evaluate JavaScript
    EvaluateScript {
        tab_id: String,
        script: String,
        await_promise: bool,
        #[serde(default)]
        frame_id: Option<String>,
    },

    /// Capture screenshot
    CaptureScreenshot {
        tab_id: String,
        format: String,
        quality: Option<u8>,
        full_page: bool,
        selector: Option<String>,
        clip_x: Option<f64>,
        clip_y: Option<f64>,
        clip_width: Option<f64>,
        clip_height: Option<f64>,
        clip_scale: Option<f64>,
    },

    /// Scroll page
    Scroll {
        tab_id: String,
        x: Option<i32>,
        y: Option<i32>,
        delta_x: Option<i32>,
        delta_y: Option<i32>,
        selector: Option<String>,
        behavior: Option<String>,
    },

    /// Find element
    FindElement {
        tab_id: String,
        selector: String,
        timeout: Option<u64>,
    },

    /// Find all elements
    FindElements {
        tab_id: String,
        selector: String,
    },

    /// Wait for element
    WaitForElement {
        tab_id: String,
        selector: String,
        timeout: u64,
        visible: bool,
    },

    /// Wait for navigation
    WaitForNavigation {
        tab_id: String,
        timeout: u64,
    },

    /// Get element attribute
    GetAttribute {
        tab_id: String,
        selector: String,
        attribute: String,
    },

    /// Set element attribute
    SetAttribute {
        tab_id: String,
        selector: String,
        attribute: String,
        value: String,
    },

    /// Get element text
    GetText {
        tab_id: String,
        selector: String,
    },

    /// Get element value
    GetValue {
        tab_id: String,
        selector: String,
    },

    /// Set element value
    SetValue {
        tab_id: String,
        selector: String,
        value: String,
    },

    /// Focus element
    Focus {
        tab_id: String,
        selector: String,
    },

    /// Blur element
    Blur {
        tab_id: String,
        selector: String,
    },

    /// Select option in dropdown
    Select {
        tab_id: String,
        selector: String,
        value: Option<String>,
        label: Option<String>,
        index: Option<usize>,
    },

    /// Check/uncheck checkbox
    SetChecked {
        tab_id: String,
        selector: String,
        checked: bool,
    },

    /// Get page URL
    GetUrl {
        tab_id: String,
    },

    /// Get page title
    GetTitle {
        tab_id: String,
    },

    /// Get page HTML
    GetHtml {
        tab_id: String,
        selector: Option<String>,
        outer: bool,
    },

    /// Get all tabs info
    GetTabs,

    /// Get active tab
    GetActiveTab,

    /// Set active tab
    SetActiveTab {
        tab_id: String,
    },

    /// Set viewport size
    SetViewport {
        tab_id: String,
        width: u32,
        height: u32,
    },

    /// Set user agent
    SetUserAgent {
        tab_id: String,
        user_agent: String,
    },

    /// Clear cookies
    ClearCookies {
        tab_id: Option<String>,
        domain: Option<String>,
    },

    /// Get cookies
    GetCookies {
        tab_id: String,
        url: Option<String>,
    },

    /// Set cookie
    SetCookie {
        tab_id: String,
        name: String,
        value: String,
        domain: Option<String>,
        path: Option<String>,
        secure: Option<bool>,
        http_only: Option<bool>,
        expires: Option<i64>,
    },

    /// Handle dialog (alert, confirm, prompt)
    HandleDialog {
        tab_id: String,
        accept: bool,
        text: Option<String>,
    },

    /// Emulate device
    EmulateDevice {
        tab_id: String,
        device_name: String,
    },

    /// Set geolocation
    SetGeolocation {
        tab_id: String,
        latitude: f64,
        longitude: f64,
        accuracy: Option<f64>,
    },

    /// Enable/disable JavaScript
    SetJavaScriptEnabled {
        tab_id: String,
        enabled: bool,
    },

    /// Get frame tree for a tab
    GetFrameTree {
        tab_id: String,
    },

    /// Evaluate JavaScript in a specific frame
    EvaluateInFrame {
        tab_id: String,
        frame_id: String,
        script: String,
        await_promise: bool,
    },

    /// Capture DOM snapshot with bounding-box information for all visible elements
    DomSnapshot {
        tab_id: String,
        #[serde(default = "default_max_nodes")]
        max_nodes: u32,
        #[serde(default = "default_include_text")]
        include_text: bool,
    },

    /// Capture annotated screenshot with numbered vision labels for KI agent interaction
    VisionAnnotated {
        tab_id: String,
        format: String,
    },

    /// Get vision labels (bounding boxes + metadata) without screenshot
    VisionLabels {
        tab_id: String,
    },

    /// Annotate screenshot with element overlays and optional OCR
    AnnotateElements {
        tab_id: String,
        #[serde(default)]
        types: Vec<String>,
        #[serde(default)]
        selector: Option<String>,
        #[serde(default)]
        ocr: bool,
        #[serde(default = "default_ocr_lang")]
        ocr_lang: String,
    },

    /// Shutdown the browser
    Shutdown,
}

fn default_ocr_lang() -> String {
    "deu+eng".to_string()
}

fn default_max_nodes() -> u32 {
    1000
}

fn default_include_text() -> bool {
    true
}

/// Response to an IPC command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcResponse {
    /// Whether the command succeeded
    pub success: bool,

    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Tab ID for tab-related responses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,

    /// Response data (JSON value)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl IpcResponse {
    /// Create a success response
    pub fn success() -> Self {
        Self {
            success: true,
            error: None,
            tab_id: None,
            data: None,
        }
    }

    /// Create a success response with tab ID
    pub fn success_with_tab(tab_id: String) -> Self {
        Self {
            success: true,
            error: None,
            tab_id: Some(tab_id),
            data: None,
        }
    }

    /// Create a success response with data
    pub fn success_with_data(data: serde_json::Value) -> Self {
        Self {
            success: true,
            error: None,
            tab_id: None,
            data: Some(data),
        }
    }

    /// Create an error response
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            error: Some(message.into()),
            tab_id: None,
            data: None,
        }
    }
}

/// IPC message wrapper with command ID
#[derive(Debug)]
pub enum IpcMessage {
    /// Command from API to browser
    Command(IpcCommand),

    /// Response from browser to API
    Response(IpcResponse),

    /// Shutdown signal
    Shutdown,
}

/// Pending command awaiting response
#[allow(dead_code)]
struct PendingCommand {
    response_tx: oneshot::Sender<IpcResponse>,
}

/// IPC channel for bidirectional communication
pub struct IpcChannel {
    /// Channel for sending commands
    command_tx: mpsc::Sender<IpcCommandMessage>,

    /// Channel for receiving commands (browser side)
    command_rx: std::sync::Arc<RwLock<Option<mpsc::Receiver<IpcCommandMessage>>>>,

    /// Default timeout for commands
    default_timeout: Duration,
}

impl Clone for IpcChannel {
    fn clone(&self) -> Self {
        Self {
            command_tx: self.command_tx.clone(),
            command_rx: self.command_rx.clone(),
            default_timeout: self.default_timeout,
        }
    }
}

impl IpcChannel {
    /// Create a new IPC channel
    pub fn new() -> Self {
        let (command_tx, command_rx) = mpsc::channel(256);

        Self {
            command_tx,
            command_rx: std::sync::Arc::new(RwLock::new(Some(command_rx))),
            default_timeout: Duration::from_secs(120),
        }
    }

    /// Create a new IPC channel with custom buffer size
    pub fn with_buffer_size(buffer_size: usize) -> Self {
        let (command_tx, command_rx) = mpsc::channel(buffer_size);

        Self {
            command_tx,
            command_rx: std::sync::Arc::new(RwLock::new(Some(command_rx))),
            default_timeout: Duration::from_secs(120),
        }
    }

    /// Set the default timeout for commands
    pub fn set_default_timeout(&mut self, timeout: Duration) {
        self.default_timeout = timeout;
    }

    /// Send a command and wait for response
    pub async fn send_command(&self, message: IpcMessage) -> Result<IpcResponse, IpcError> {
        self.send_command_timeout(message, self.default_timeout).await
    }

    /// Send a command with custom timeout
    pub async fn send_command_timeout(
        &self,
        message: IpcMessage,
        timeout: Duration,
    ) -> Result<IpcResponse, IpcError> {
        let command = match message {
            IpcMessage::Command(cmd) => cmd,
            IpcMessage::Shutdown => {
                // Special handling for shutdown
                let (response_tx, _response_rx) = oneshot::channel();
                let command_id = NEXT_COMMAND_ID.fetch_add(1, Ordering::SeqCst);

                self.command_tx
                    .send((command_id, IpcCommand::Shutdown, response_tx))
                    .await
                    .map_err(|_| IpcError::ChannelClosed)?;

                return Ok(IpcResponse::success());
            }
            IpcMessage::Response(_) => {
                return Err(IpcError::InvalidMessage("Cannot send response as command".to_string()));
            }
        };

        let (response_tx, response_rx) = oneshot::channel();
        let command_id = NEXT_COMMAND_ID.fetch_add(1, Ordering::SeqCst);

        debug!("Sending IPC command {}: {:?}", command_id, command);

        self.command_tx
            .send((command_id, command, response_tx))
            .await
            .map_err(|_| IpcError::ChannelClosed)?;

        // Wait for response with timeout
        match tokio::time::timeout(timeout, response_rx).await {
            Ok(Ok(response)) => {
                debug!("Received IPC response for command {}: {:?}", command_id, response);
                Ok(response)
            }
            Ok(Err(_)) => {
                error!("IPC response channel closed for command {}", command_id);
                Err(IpcError::ChannelClosed)
            }
            Err(_) => {
                warn!("IPC command {} timed out after {:?}", command_id, timeout);
                Err(IpcError::Timeout)
            }
        }
    }

    /// Take the command receiver (for the browser side)
    pub async fn take_receiver(&self) -> Option<mpsc::Receiver<IpcCommandMessage>> {
        self.command_rx.write().await.take()
    }

    /// Check if the channel is still open
    pub fn is_open(&self) -> bool {
        !self.command_tx.is_closed()
    }
}

impl Default for IpcChannel {
    fn default() -> Self {
        Self::new()
    }
}

/// IPC error types
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("IPC channel closed")]
    ChannelClosed,

    #[error("Command timed out")]
    Timeout,

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("Command failed: {0}")]
    CommandFailed(String),
}

/// Helper to process IPC commands on the browser side
pub struct IpcProcessor {
    /// Receiver for commands
    receiver: mpsc::Receiver<IpcCommandMessage>,
}

impl IpcProcessor {
    /// Create a new processor from a channel
    pub async fn new(channel: &IpcChannel) -> Option<Self> {
        channel.take_receiver().await.map(|receiver| Self { receiver })
    }

    /// Receive the next command
    pub async fn recv(&mut self) -> Option<IpcCommandMessage> {
        self.receiver.recv().await
    }

    /// Process commands with a handler function (sequentially)
    pub async fn process<F, Fut>(&mut self, mut handler: F)
    where
        F: FnMut(IpcCommand) -> Fut,
        Fut: std::future::Future<Output = IpcResponse>,
    {
        while let Some((command_id, command, response_tx)) = self.receiver.recv().await {
            debug!("Processing IPC command {}: {:?}", command_id, command);

            let response = handler(command).await;

            if response_tx.send(response).is_err() {
                warn!("Failed to send response for command {}", command_id);
            }
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_response_success() {
        let response = IpcResponse::success();
        assert!(response.success);
        assert!(response.error.is_none());
    }

    #[test]
    fn test_ipc_response_error() {
        let response = IpcResponse::error("Something went wrong");
        assert!(!response.success);
        assert_eq!(response.error, Some("Something went wrong".to_string()));
    }

    #[test]
    fn test_ipc_command_serialization() {
        let command = IpcCommand::Navigate {
            tab_id: "tab_1".to_string(),
            url: "https://example.com".to_string(),
        };

        let json = serde_json::to_string(&command).unwrap();
        assert!(json.contains("Navigate"));
        assert!(json.contains("tab_1"));
    }

    #[tokio::test]
    async fn test_ipc_channel_creation() {
        let channel = IpcChannel::new();
        assert!(channel.is_open());
    }

    #[tokio::test]
    async fn test_ipc_channel_timeout() {
        let channel = IpcChannel::new();

        // Don't take the receiver, so commands will timeout
        let result = channel
            .send_command_timeout(
                IpcMessage::Command(IpcCommand::GetTabs),
                Duration::from_millis(100),
            )
            .await;

        assert!(matches!(result, Err(IpcError::Timeout)));
    }

    #[tokio::test]
    async fn test_ipc_round_trip() {
        let channel = IpcChannel::new();

        // Take the receiver
        let mut receiver = channel.take_receiver().await.unwrap();

        // Spawn a task to handle commands
        let handler = tokio::spawn(async move {
            if let Some((_id, cmd, tx)) = receiver.recv().await {
                match cmd {
                    IpcCommand::GetTabs => {
                        let _ = tx.send(IpcResponse::success_with_data(
                            serde_json::json!({ "tabs": [] }),
                        ));
                    }
                    _ => {
                        let _ = tx.send(IpcResponse::error("Unknown command"));
                    }
                }
            }
        });

        // Send a command
        let response = channel
            .send_command(IpcMessage::Command(IpcCommand::GetTabs))
            .await
            .unwrap();

        assert!(response.success);

        handler.await.unwrap();
    }
}
