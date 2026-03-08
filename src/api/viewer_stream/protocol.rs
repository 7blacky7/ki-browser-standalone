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
