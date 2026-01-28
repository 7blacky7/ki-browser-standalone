//! WebSocket handler for real-time browser events
//!
//! Provides WebSocket connectivity for broadcasting browser events
//! and receiving commands from connected clients.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, info, warn};

use crate::api::server::AppState;

/// Unique client identifier
static NEXT_CLIENT_ID: AtomicU64 = AtomicU64::new(1);

/// Browser events that can be broadcast to connected clients
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum BrowserEvent {
    /// A new tab was created
    TabCreated {
        tab_id: String,
        url: String,
    },

    /// A tab was closed
    TabClosed {
        tab_id: String,
    },

    /// Navigation completed in a tab
    NavigationComplete {
        tab_id: String,
        url: String,
        title: String,
    },

    /// DOM is ready in a tab
    DomReady {
        tab_id: String,
    },

    /// Page finished loading
    LoadComplete {
        tab_id: String,
        url: String,
    },

    /// Tab title changed
    TitleChanged {
        tab_id: String,
        title: String,
    },

    /// Tab URL changed (without full navigation)
    UrlChanged {
        tab_id: String,
        url: String,
    },

    /// Tab favicon changed
    FaviconChanged {
        tab_id: String,
        favicon_url: Option<String>,
    },

    /// Tab loading state changed
    LoadingStateChanged {
        tab_id: String,
        is_loading: bool,
    },

    /// Active tab changed
    ActiveTabChanged {
        tab_id: String,
    },

    /// Console message from a tab
    ConsoleMessage {
        tab_id: String,
        level: String, // "log", "warn", "error", "info", "debug"
        message: String,
        source: Option<String>,
        line: Option<u32>,
    },

    /// JavaScript dialog appeared
    DialogOpened {
        tab_id: String,
        dialog_type: String, // "alert", "confirm", "prompt", "beforeunload"
        message: String,
    },

    /// Download started
    DownloadStarted {
        download_id: String,
        url: String,
        filename: String,
    },

    /// Download progress updated
    DownloadProgress {
        download_id: String,
        received_bytes: u64,
        total_bytes: Option<u64>,
    },

    /// Download completed
    DownloadComplete {
        download_id: String,
        path: String,
    },

    /// An error occurred
    Error {
        tab_id: Option<String>,
        code: String,
        message: String,
    },

    /// Connection established (sent to new clients)
    Connected {
        client_id: u64,
        server_version: String,
    },

    /// Ping for keepalive
    Ping {
        timestamp: u64,
    },

    /// Pong response
    Pong {
        timestamp: u64,
    },
}

/// Commands that can be received via WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WebSocketCommand {
    /// Subscribe to specific event types
    Subscribe {
        events: Vec<String>,
    },

    /// Unsubscribe from specific event types
    Unsubscribe {
        events: Vec<String>,
    },

    /// Ping request
    Ping {
        timestamp: u64,
    },
}

/// WebSocket message wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessage {
    /// Message ID for correlation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// The event or command
    #[serde(flatten)]
    pub payload: WebSocketPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WebSocketPayload {
    Event(BrowserEvent),
    Command(WebSocketCommand),
}

/// Connected client information
#[derive(Debug)]
struct ClientInfo {
    id: u64,
    subscribed_events: Vec<String>,
    tx: mpsc::Sender<BrowserEvent>,
}

/// WebSocket handler for managing connections and broadcasting events
pub struct WebSocketHandler {
    /// Broadcast channel for events
    broadcast_tx: broadcast::Sender<BrowserEvent>,

    /// Connected clients
    clients: RwLock<HashMap<u64, ClientInfo>>,

    /// Ping interval in seconds
    ping_interval: Duration,
}

impl WebSocketHandler {
    /// Create a new WebSocket handler
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(1024);

        Self {
            broadcast_tx,
            clients: RwLock::new(HashMap::new()),
            ping_interval: Duration::from_secs(30),
        }
    }

    /// Create a new WebSocket handler with custom ping interval
    pub fn with_ping_interval(ping_interval: Duration) -> Self {
        let (broadcast_tx, _) = broadcast::channel(1024);

        Self {
            broadcast_tx,
            clients: RwLock::new(HashMap::new()),
            ping_interval,
        }
    }

    /// Broadcast an event to all connected clients
    pub async fn broadcast(&self, event: BrowserEvent) {
        let clients = self.clients.read().await;
        let event_type = Self::event_type_name(&event);

        for client in clients.values() {
            // Check if client is subscribed to this event type
            if client.subscribed_events.is_empty()
                || client.subscribed_events.contains(&"*".to_string())
                || client.subscribed_events.contains(&event_type)
            {
                if let Err(e) = client.tx.send(event.clone()).await {
                    warn!("Failed to send event to client {}: {}", client.id, e);
                }
            }
        }

        // Also send through broadcast channel
        let _ = self.broadcast_tx.send(event);
    }

    /// Get the event type name for filtering
    fn event_type_name(event: &BrowserEvent) -> String {
        match event {
            BrowserEvent::TabCreated { .. } => "TabCreated".to_string(),
            BrowserEvent::TabClosed { .. } => "TabClosed".to_string(),
            BrowserEvent::NavigationComplete { .. } => "NavigationComplete".to_string(),
            BrowserEvent::DomReady { .. } => "DomReady".to_string(),
            BrowserEvent::LoadComplete { .. } => "LoadComplete".to_string(),
            BrowserEvent::TitleChanged { .. } => "TitleChanged".to_string(),
            BrowserEvent::UrlChanged { .. } => "UrlChanged".to_string(),
            BrowserEvent::FaviconChanged { .. } => "FaviconChanged".to_string(),
            BrowserEvent::LoadingStateChanged { .. } => "LoadingStateChanged".to_string(),
            BrowserEvent::ActiveTabChanged { .. } => "ActiveTabChanged".to_string(),
            BrowserEvent::ConsoleMessage { .. } => "ConsoleMessage".to_string(),
            BrowserEvent::DialogOpened { .. } => "DialogOpened".to_string(),
            BrowserEvent::DownloadStarted { .. } => "DownloadStarted".to_string(),
            BrowserEvent::DownloadProgress { .. } => "DownloadProgress".to_string(),
            BrowserEvent::DownloadComplete { .. } => "DownloadComplete".to_string(),
            BrowserEvent::Error { .. } => "Error".to_string(),
            BrowserEvent::Connected { .. } => "Connected".to_string(),
            BrowserEvent::Ping { .. } => "Ping".to_string(),
            BrowserEvent::Pong { .. } => "Pong".to_string(),
        }
    }

    /// Subscribe to the broadcast channel
    pub fn subscribe(&self) -> broadcast::Receiver<BrowserEvent> {
        self.broadcast_tx.subscribe()
    }

    /// Get the number of connected clients
    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }

    /// Add a new client
    async fn add_client(&self, tx: mpsc::Sender<BrowserEvent>) -> u64 {
        let client_id = NEXT_CLIENT_ID.fetch_add(1, Ordering::SeqCst);

        let client = ClientInfo {
            id: client_id,
            subscribed_events: vec![], // Empty means all events
            tx,
        };

        self.clients.write().await.insert(client_id, client);

        info!("WebSocket client {} connected", client_id);

        client_id
    }

    /// Remove a client
    async fn remove_client(&self, client_id: u64) {
        self.clients.write().await.remove(&client_id);
        info!("WebSocket client {} disconnected", client_id);
    }

    /// Update client subscriptions
    async fn subscribe_client(&self, client_id: u64, events: Vec<String>) {
        if let Some(client) = self.clients.write().await.get_mut(&client_id) {
            for event in events {
                if !client.subscribed_events.contains(&event) {
                    client.subscribed_events.push(event);
                }
            }
            debug!("Client {} subscribed to: {:?}", client_id, client.subscribed_events);
        }
    }

    /// Remove client subscriptions
    async fn unsubscribe_client(&self, client_id: u64, events: Vec<String>) {
        if let Some(client) = self.clients.write().await.get_mut(&client_id) {
            client.subscribed_events.retain(|e| !events.contains(e));
            debug!("Client {} unsubscribed, now subscribed to: {:?}", client_id, client.subscribed_events);
        }
    }
}

impl Default for WebSocketHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// WebSocket upgrade handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handle an individual WebSocket connection
async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Create channel for sending events to this client
    let (tx, mut rx) = mpsc::channel::<BrowserEvent>(256);

    // Register client
    let client_id = state.ws_handler.add_client(tx).await;

    // Send connected event
    let connected_event = BrowserEvent::Connected {
        client_id,
        server_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let connected_msg = serde_json::to_string(&WebSocketMessage {
        id: None,
        payload: WebSocketPayload::Event(connected_event),
    }).unwrap();

    if sender.send(Message::Text(connected_msg)).await.is_err() {
        state.ws_handler.remove_client(client_id).await;
        return;
    }

    let ws_handler = state.ws_handler.clone();
    let ping_interval = ws_handler.ping_interval;

    // Task to send events to client
    let mut send_task = tokio::spawn(async move {
        let mut ping_timer = tokio::time::interval(ping_interval);

        loop {
            tokio::select! {
                // Send events from channel
                Some(event) = rx.recv() => {
                    let msg = serde_json::to_string(&WebSocketMessage {
                        id: None,
                        payload: WebSocketPayload::Event(event),
                    }).unwrap();

                    if sender.send(Message::Text(msg)).await.is_err() {
                        break;
                    }
                }

                // Send periodic pings
                _ = ping_timer.tick() => {
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;

                    let ping_event = BrowserEvent::Ping { timestamp };
                    let msg = serde_json::to_string(&WebSocketMessage {
                        id: None,
                        payload: WebSocketPayload::Event(ping_event),
                    }).unwrap();

                    if sender.send(Message::Text(msg)).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    let ws_handler_recv = state.ws_handler.clone();

    // Task to receive messages from client
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    // Try to parse as WebSocket command
                    match serde_json::from_str::<WebSocketMessage>(&text) {
                        Ok(ws_msg) => {
                            if let WebSocketPayload::Command(cmd) = ws_msg.payload {
                                match cmd {
                                    WebSocketCommand::Subscribe { events } => {
                                        ws_handler_recv.subscribe_client(client_id, events).await;
                                    }
                                    WebSocketCommand::Unsubscribe { events } => {
                                        ws_handler_recv.unsubscribe_client(client_id, events).await;
                                    }
                                    WebSocketCommand::Ping { timestamp } => {
                                        // Pong is handled by the send task via broadcast
                                        ws_handler_recv.broadcast(BrowserEvent::Pong { timestamp }).await;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            debug!("Failed to parse WebSocket message: {}", e);
                        }
                    }
                }
                Message::Binary(_) => {
                    // Binary messages not supported
                    debug!("Received unsupported binary message");
                }
                Message::Ping(data) => {
                    // WebSocket protocol ping - handled automatically by axum
                    debug!("Received WebSocket ping");
                }
                Message::Pong(_) => {
                    // WebSocket protocol pong
                    debug!("Received WebSocket pong");
                }
                Message::Close(_) => {
                    break;
                }
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
        }
        _ = &mut recv_task => {
            send_task.abort();
        }
    }

    // Clean up
    state.ws_handler.remove_client(client_id).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_event_serialization() {
        let event = BrowserEvent::TabCreated {
            tab_id: "tab_1".to_string(),
            url: "https://example.com".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("TabCreated"));
        assert!(json.contains("tab_1"));
    }

    #[test]
    fn test_websocket_command_deserialization() {
        let json = r#"{"type":"Subscribe","data":{"events":["TabCreated","TabClosed"]}}"#;
        let cmd: WebSocketCommand = serde_json::from_str(json).unwrap();

        match cmd {
            WebSocketCommand::Subscribe { events } => {
                assert_eq!(events.len(), 2);
                assert!(events.contains(&"TabCreated".to_string()));
            }
            _ => panic!("Expected Subscribe command"),
        }
    }

    #[tokio::test]
    async fn test_websocket_handler_client_count() {
        let handler = WebSocketHandler::new();
        assert_eq!(handler.client_count().await, 0);
    }
}
