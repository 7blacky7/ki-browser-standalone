//! WebSocket message protocol for communicating with the ki-browser /ws/viewer endpoint.
//!
//! Mirrors the server-side protocol types. Server sends JSON text messages
//! for control and binary messages for JPEG frames. Client sends JSON input events.

use serde::{Deserialize, Serialize};

/// Messages received from the server (JSON text messages).
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// Initial handshake confirming connection with current tab state.
    #[serde(rename = "connected")]
    Connected {
        active_tab: Option<String>,
        tabs: Vec<TabInfo>,
    },
    /// Tab list or active tab changed.
    #[serde(rename = "tab_update")]
    TabUpdate {
        active_tab: Option<String>,
        tabs: Vec<TabInfo>,
    },
    /// Server-side error.
    #[serde(rename = "error")]
    Error { message: String },
}

/// Tab metadata sent by the server.
#[derive(Debug, Clone, Deserialize)]
pub struct TabInfo {
    pub id: String,
    pub url: String,
    pub title: String,
}

/// Messages sent from client to server (JSON text).
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "mouse_move")]
    MouseMove { x: i32, y: i32 },
    #[serde(rename = "mouse_click")]
    MouseClick { x: i32, y: i32, button: i32 },
    #[serde(rename = "mouse_wheel")]
    MouseWheel {
        x: i32,
        y: i32,
        delta_x: i32,
        delta_y: i32,
    },
    #[serde(rename = "key_event")]
    KeyEvent {
        event_type: i32,
        modifiers: u32,
        windows_key_code: i32,
        #[serde(default)]
        character: u16,
    },
    #[serde(rename = "type_text")]
    TypeText { text: String },
    #[serde(rename = "navigate")]
    Navigate { url: String },
    #[serde(rename = "create_tab")]
    CreateTab { url: String },
    #[serde(rename = "close_tab")]
    CloseTab { tab_id: String },
    #[serde(rename = "set_active_tab")]
    SetActiveTab { tab_id: String },
    #[serde(rename = "resize")]
    Resize { width: u32, height: u32 },
    #[serde(rename = "go_back")]
    GoBack,
    #[serde(rename = "go_forward")]
    GoForward,
}
