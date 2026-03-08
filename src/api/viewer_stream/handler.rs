//! WebSocket handler for /ws/viewer — streams encoded frames and forwards input.
//!
//! Polls the CEF frame buffer at ~30fps, encodes changed frames (JPEG or H.264
//! NVENC when the h264 feature is enabled), and sends them as binary WebSocket
//! messages. Receives input events (mouse, keyboard) from the client and
//! forwards them to the CEF engine.
//! Supports multi-tab switching and sends tab-state updates on changes.

#[cfg(feature = "cef-browser")]
use crate::browser::cef_engine::CefBrowserEngine;
use crate::api::server::AppState;
use crate::api::viewer_stream::encoder::{self, FrameEncoder};
use crate::api::viewer_stream::protocol::{ClientMessage, ServerMessage, TabInfo, tab_id_str};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info};
use uuid::Uuid;

/// Axum handler for WebSocket upgrade on /ws/viewer.
pub async fn viewer_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_viewer_socket(socket, state))
}

/// Main viewer WebSocket loop: frame streaming + input forwarding.
async fn handle_viewer_socket(socket: WebSocket, state: AppState) {
    #[cfg(not(feature = "cef-browser"))]
    {
        let err = ServerMessage::Error {
            message: "CEF engine not available".into(),
        };
        let mut socket = socket;
        let _ = socket
            .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
            .await;
        return;
    }

    #[cfg(feature = "cef-browser")]
    {
        let engine = match &state.cef_engine {
            Some(e) => e.clone(),
            None => {
                let err = ServerMessage::Error {
                    message: "CEF engine not attached to AppState".into(),
                };
                let mut socket = socket;
                let _ = socket
                    .send(Message::Text(serde_json::to_string(&err).unwrap().into()))
                    .await;
                return;
            }
        };

        info!("Viewer client connected");
        let (mut ws_sender, mut ws_receiver) = socket.split();

        // Determine initial active tab and build tab list.
        let tabs_info = build_tab_list(&engine);
        let initial_active = engine.get_tabs_sync().first().map(|t| t.id);
        let active_tab_id: Arc<parking_lot::Mutex<Option<Uuid>>> =
            Arc::new(parking_lot::Mutex::new(initial_active));

        // Send initial Connected message with tab list.
        let connected = ServerMessage::Connected {
            active_tab: initial_active.map(tab_id_str),
            tabs: tabs_info,
        };
        if ws_sender
            .send(Message::Text(
                serde_json::to_string(&connected).unwrap().into(),
            ))
            .await
            .is_err()
        {
            return;
        }

        // Channel for sending tab-update messages from recv_task to send_task.
        let (tab_update_tx, mut tab_update_rx) =
            tokio::sync::mpsc::unbounded_channel::<ServerMessage>();

        // Frame sending task: poll frame buffer, encode, and forward tab updates.
        let send_engine = engine.clone();
        let send_active = active_tab_id.clone();
        let send_task = tokio::spawn(async move {
            let mut last_version: u64 = 0;
            let mut last_tab_snapshot = build_tab_snapshot(&send_engine);
            let mut frame_encoder: Option<Box<dyn FrameEncoder>> = None;
            let mut interval = tokio::time::interval(Duration::from_millis(33)); // ~30fps
            let mut last_force_sent = Instant::now();
            loop {
                // Check for pending tab-update messages first (non-blocking).
                while let Ok(update_msg) = tab_update_rx.try_recv() {
                    let json = serde_json::to_string(&update_msg).unwrap();
                    if ws_sender.send(Message::Text(json.into())).await.is_err() {
                        return;
                    }
                }

                interval.tick().await;

                // Detect tab-list changes and send TabUpdate.
                let current_snapshot = build_tab_snapshot(&send_engine);
                if current_snapshot != last_tab_snapshot {
                    let active = send_active.lock().map(tab_id_str);
                    let tabs_info = snapshot_to_tab_info(&current_snapshot);
                    let update = ServerMessage::TabUpdate {
                        active_tab: active,
                        tabs: tabs_info,
                    };
                    let json = serde_json::to_string(&update).unwrap();
                    if ws_sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                    last_tab_snapshot = current_snapshot;
                }

                // Periodically force re-encode to flush H.264 decoder buffer on client.
                // The cuvid decoder buffers the first packet and only outputs a frame
                // once a second packet arrives; this ensures static pages are visible.
                let force_refresh = last_force_sent.elapsed() > Duration::from_secs(1);

                // Read frame buffer from active tab.
                let current_active = *send_active.lock();
                let messages = encode_frame_if_new(
                    &send_engine,
                    current_active,
                    &mut last_version,
                    &mut frame_encoder,
                    force_refresh,
                );
                if !messages.is_empty() && force_refresh {
                    last_force_sent = Instant::now();
                }
                for data in messages {
                    if ws_sender
                        .send(Message::Binary(data.into()))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
        });

        // Input receiving task: parse client messages and forward to CEF.
        let recv_engine = engine.clone();
        let recv_active = active_tab_id.clone();
        let recv_task = tokio::spawn(async move {
            while let Some(Ok(msg)) = ws_receiver.next().await {
                match msg {
                    Message::Text(text) => {
                        handle_client_message(
                            &text,
                            &recv_engine,
                            &recv_active,
                            &tab_update_tx,
                        );
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        });

        // Wait for either task to finish (client disconnect or error).
        tokio::select! {
            _ = send_task => {},
            _ = recv_task => {},
        }

        info!("Viewer client disconnected");
    }
}

/// Encode the current frame buffer if the frame version changed or force is set.
/// Lazily initializes the encoder on first frame (needs dimensions).
/// When `force` is true, re-encodes the current frame even if frame_version has
/// not changed — this flushes the H.264 decoder buffer on the client side, which
/// requires at least two packets before producing output.
/// Returns zero or more binary messages (prefixed with codec byte).
#[cfg(feature = "cef-browser")]
fn encode_frame_if_new(
    engine: &Arc<CefBrowserEngine>,
    active_tab: Option<Uuid>,
    last_version: &mut u64,
    frame_encoder: &mut Option<Box<dyn FrameEncoder>>,
    force: bool,
) -> Vec<Vec<u8>> {
    let tab_id = match active_tab.or_else(|| engine.get_tabs_sync().first().map(|t| t.id)) {
        Some(id) => id,
        None => return Vec::new(),
    };
    let (fb_arc, size_arc, version_arc) = match engine.get_tab_frame_buffer(tab_id) {
        Some(bufs) => bufs,
        None => return Vec::new(),
    };

    let current = version_arc.load(Ordering::Acquire);
    if current == *last_version && !force {
        return Vec::new();
    }
    *last_version = current;

    // Read frame buffer under lock.
    let fb = fb_arc.read();
    let (w, h) = *size_arc.read();
    if w == 0 || h == 0 || fb.is_empty() {
        return Vec::new();
    }

    // Lazily create encoder with actual frame dimensions.
    if frame_encoder.is_none() {
        let enc = encoder::create_encoder(w, h);
        // Send codec config if available (e.g., H.264 SPS/PPS).
        let config = enc.codec_config();
        *frame_encoder = Some(enc);
        if let Some(config_data) = config {
            // Config will be sent as first message before frames.
            let mut messages = vec![config_data];
            messages.extend(frame_encoder.as_mut().unwrap().encode(&fb, w, h));
            return messages;
        }
    }

    let enc = frame_encoder.as_mut().unwrap();
    enc.encode(&fb, w, h)
}

/// Process a client input message and forward it to the CEF engine.
/// Handles SetActiveTab by updating the shared active tab tracker.
#[cfg(feature = "cef-browser")]
fn handle_client_message(
    text: &str,
    engine: &Arc<CefBrowserEngine>,
    active_tab_id: &Arc<parking_lot::Mutex<Option<Uuid>>>,
    tab_update_tx: &tokio::sync::mpsc::UnboundedSender<ServerMessage>,
) {
    let msg: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            debug!("Invalid viewer message: {e}");
            return;
        }
    };

    // Resolve active tab, falling back to first tab.
    let resolve_active = || -> Option<Uuid> {
        active_tab_id
            .lock()
            .or_else(|| engine.get_tabs_sync().first().map(|t| t.id))
    };

    match msg {
        ClientMessage::SetActiveTab { tab_id } => {
            if let Ok(uuid) = Uuid::parse_str(&tab_id) {
                // Verify the tab exists before switching.
                let tabs = engine.get_tabs_sync();
                if tabs.iter().any(|t| t.id == uuid) {
                    *active_tab_id.lock() = Some(uuid);
                    info!("Viewer switched to tab {uuid}");
                    // Notify client of tab switch.
                    let tabs_info = build_tab_list(engine);
                    let update = ServerMessage::TabUpdate {
                        active_tab: Some(tab_id_str(uuid)),
                        tabs: tabs_info,
                    };
                    let _ = tab_update_tx.send(update);
                } else {
                    debug!("SetActiveTab: tab {tab_id} not found");
                }
            }
        }
        ClientMessage::MouseMove { x, y } => {
            if let Some(tab) = resolve_active() {
                engine.send_mouse_move(tab, x, y);
            }
        }
        ClientMessage::MouseClick { x, y, button } => {
            if let Some(tab) = resolve_active() {
                engine.send_mouse_click(tab, x, y, button);
            }
        }
        ClientMessage::MouseWheel {
            x,
            y,
            delta_x,
            delta_y,
        } => {
            if let Some(tab) = resolve_active() {
                engine.send_mouse_wheel(tab, x, y, delta_x, delta_y);
            }
        }
        ClientMessage::KeyEvent {
            event_type,
            modifiers,
            windows_key_code,
            character,
        } => {
            if let Some(tab) = resolve_active() {
                engine.send_key_event(tab, event_type, modifiers, windows_key_code, character);
            }
        }
        ClientMessage::TypeText { text } => {
            if let Some(tab) = resolve_active() {
                engine.send_type_text(tab, &text);
            }
        }
        ClientMessage::Navigate { url } => {
            if let Some(tab) = resolve_active() {
                engine.send_navigate(tab, &url);
            }
        }
        ClientMessage::CreateTab { url } => {
            let new_id = engine.send_create_tab(&url);
            // Auto-switch to the newly created tab.
            *active_tab_id.lock() = Some(new_id);
        }
        ClientMessage::CloseTab { tab_id } => {
            if let Ok(uuid) = Uuid::parse_str(&tab_id) {
                engine.send_close_tab(uuid);
                // If closing the active tab, switch to first remaining tab.
                let mut active = active_tab_id.lock();
                if *active == Some(uuid) {
                    *active = engine
                        .get_tabs_sync()
                        .iter()
                        .find(|t| t.id != uuid)
                        .map(|t| t.id);
                }
            }
        }
        ClientMessage::Resize { width, height } => {
            if let Some(tab) = resolve_active() {
                engine.send_resize_viewport(tab, width, height);
            }
        }
        ClientMessage::GoBack => {
            if let Some(tab) = resolve_active() {
                engine.send_go_back(tab);
            }
        }
        ClientMessage::GoForward => {
            if let Some(tab) = resolve_active() {
                engine.send_go_forward(tab);
            }
        }
    }
}

/// Lightweight tab snapshot for change detection (id, url, title).
#[cfg(feature = "cef-browser")]
type TabSnapshot = Vec<(String, String, String)>;

/// Build a comparable snapshot of the current tab list.
#[cfg(feature = "cef-browser")]
fn build_tab_snapshot(engine: &Arc<CefBrowserEngine>) -> TabSnapshot {
    engine
        .get_tabs_sync()
        .iter()
        .map(|t| (tab_id_str(t.id), t.url.clone(), t.title.clone()))
        .collect()
}

/// Convert a tab snapshot into TabInfo vec for protocol messages.
#[cfg(feature = "cef-browser")]
fn snapshot_to_tab_info(snapshot: &TabSnapshot) -> Vec<TabInfo> {
    snapshot
        .iter()
        .map(|(id, url, title)| TabInfo {
            id: id.clone(),
            url: url.clone(),
            title: title.clone(),
        })
        .collect()
}

/// Build a list of TabInfo from the engine's current tabs.
#[cfg(feature = "cef-browser")]
fn build_tab_list(engine: &Arc<CefBrowserEngine>) -> Vec<TabInfo> {
    engine
        .get_tabs_sync()
        .iter()
        .map(|t| TabInfo {
            id: tab_id_str(t.id),
            url: t.url.clone(),
            title: t.title.clone(),
        })
        .collect()
}
