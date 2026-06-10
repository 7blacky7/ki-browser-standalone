//! WebSocket message protocol for the /ws/viewer video-stream endpoint.
//!
//! Defines all message types exchanged between viewer client and server:
//! server sends JPEG frames + tab state, client sends input events.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Messages sent from server to viewer client.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    /// Initial handshake confirming connection.
    #[serde(rename = "connected")]
    Connected {
        active_tab: Option<String>,
        tabs: Vec<TabInfo>,
    },
    /// Tab list changed (created, closed, switched).
    #[serde(rename = "tab_update")]
    TabUpdate {
        active_tab: Option<String>,
        tabs: Vec<TabInfo>,
    },
    /// Server-side error.
    #[serde(rename = "error")]
    Error { message: String },
    /// Answer to `ClientMessage::GetPageContext`: link under the queried
    /// point and the page's current text selection (both optional).
    #[serde(rename = "page_context")]
    PageContext {
        request_id: u32,
        link: Option<String>,
        selection: Option<String>,
    },
}

/// Minimal tab info sent to the viewer client.
#[derive(Debug, Clone, Serialize)]
pub struct TabInfo {
    pub id: String,
    pub url: String,
    pub title: String,
}

/// Messages sent from viewer client to server.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "mouse_move")]
    MouseMove { x: i32, y: i32 },
    #[serde(rename = "mouse_click")]
    MouseClick { x: i32, y: i32, button: i32 },
    /// Single button transition for real press/drag/release sequences
    /// (text selection, drag & drop). `click_count` carries the click detail
    /// (1 = single, 2 = double click).
    #[serde(rename = "mouse_down")]
    MouseDown {
        x: i32,
        y: i32,
        button: i32,
        #[serde(default = "default_click_count")]
        click_count: i32,
    },
    #[serde(rename = "mouse_up")]
    MouseUp {
        x: i32,
        y: i32,
        button: i32,
        #[serde(default = "default_click_count")]
        click_count: i32,
    },
    /// Ask the server what is at a viewport point (link under the cursor,
    /// current text selection). Answered with `ServerMessage::PageContext`;
    /// used for the viewer context menu and the copy-to-clipboard bridge.
    #[serde(rename = "get_page_context")]
    GetPageContext {
        x: i32,
        y: i32,
        #[serde(default)]
        request_id: u32,
    },
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

fn default_click_count() -> i32 {
    1
}

/// Convert a Uuid to the string format used in TabInfo.
pub fn tab_id_str(id: Uuid) -> String {
    id.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_message_deserialize_mouse_move() {
        let json = r#"{"type":"mouse_move","x":100,"y":200}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::MouseMove { x, y } => {
                assert_eq!(x, 100);
                assert_eq!(y, 200);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_client_message_deserialize_key_event() {
        let json = r#"{"type":"key_event","event_type":0,"modifiers":0,"windows_key_code":65}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::KeyEvent {
                event_type,
                character,
                ..
            } => {
                assert_eq!(event_type, 0);
                assert_eq!(character, 0); // default
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_client_message_deserialize_mouse_down_defaults() {
        let json = r#"{"type":"mouse_down","x":5,"y":6,"button":0}"#;
        let msg: ClientMessage = serde_json::from_str(json).unwrap();
        match msg {
            ClientMessage::MouseDown { click_count, button, .. } => {
                assert_eq!(click_count, 1);
                assert_eq!(button, 0);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_server_message_serialize_page_context() {
        let msg = ServerMessage::PageContext {
            request_id: 7,
            link: Some("https://example.com".into()),
            selection: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"page_context\""));
        assert!(json.contains("\"request_id\":7"));
    }

    #[test]
    fn test_server_message_serialize_connected() {
        let msg = ServerMessage::Connected {
            active_tab: Some("abc".into()),
            tabs: vec![TabInfo {
                id: "abc".into(),
                url: "https://example.com".into(),
                title: "Example".into(),
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"connected\""));
        assert!(json.contains("\"active_tab\":\"abc\""));
    }
}
