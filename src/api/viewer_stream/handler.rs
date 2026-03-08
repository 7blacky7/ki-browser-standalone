//! WebSocket handler for /ws/viewer — streams JPEG frames and forwards input.
//!
//! Polls the CEF frame buffer at ~30fps, encodes changed frames as JPEG,
//! and sends them as binary WebSocket messages. Receives input events
//! (mouse, keyboard) from the client and forwards them to the CEF engine.

#[cfg(feature = "cef-browser")]
use crate::browser::cef_engine::CefBrowserEngine;
use crate::api::server::AppState;
use crate::api::viewer_stream::protocol::{ClientMessage, ServerMessage, TabInfo, tab_id_str};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};
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

        // Send initial Connected message with tab list.
        let tabs_info = build_tab_list(&engine);
        let active = engine
            .get_tabs_sync()
            .first()
            .map(|t| tab_id_str(t.id));
        let connected = ServerMessage::Connected {
            active_tab: active,
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

        // Frame sending task: poll frame buffer and send JPEG.
        let send_engine = engine.clone();
        let send_task = tokio::spawn(async move {
            let mut last_version: u64 = 0;
            let mut interval = tokio::time::interval(Duration::from_millis(33)); // ~30fps
            loop {
                interval.tick().await;
                match encode_frame_if_new(&send_engine, &mut last_version) {
                    Some(jpeg_data) => {
                        if ws_sender
                            .send(Message::Binary(jpeg_data.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    None => continue,
                }
            }
        });

        // Input receiving task: parse client messages and forward to CEF.
        let recv_engine = engine.clone();
        let recv_task = tokio::spawn(async move {
            while let Some(Ok(msg)) = ws_receiver.next().await {
                match msg {
                    Message::Text(text) => {
                        handle_client_message(&text, &recv_engine);
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

/// Encode the current frame buffer as JPEG if the frame version changed.
#[cfg(feature = "cef-browser")]
fn encode_frame_if_new(engine: &Arc<CefBrowserEngine>, last_version: &mut u64) -> Option<Vec<u8>> {
    // Find active tab (first tab for now).
    let tabs = engine.get_tabs_sync();
    let tab = tabs.first()?;
    let (fb_arc, size_arc, version_arc) = engine.get_tab_frame_buffer(tab.id)?;

    let current = version_arc.load(Ordering::Acquire);
    if current == *last_version {
        return None;
    }
    *last_version = current;

    // Read frame buffer under lock.
    let fb = fb_arc.read();
    let (w, h) = *size_arc.read();
    if w == 0 || h == 0 || fb.is_empty() {
        return None;
    }

    // Convert BGRA → RGB for JPEG encoding.
    let expected = (w as usize) * (h as usize) * 4;
    let len = fb.len().min(expected);
    let mut rgb = Vec::with_capacity((w as usize) * (h as usize) * 3);
    for chunk in fb[..len].chunks_exact(4) {
        rgb.push(chunk[2]); // R
        rgb.push(chunk[1]); // G
        rgb.push(chunk[0]); // B
    }
    drop(fb); // Release read lock.

    // Encode as JPEG (quality 75 — good balance of size vs quality).
    let img = image::ImageBuffer::<image::Rgb<u8>, _>::from_raw(w, h, rgb)?;
    let mut buf = Vec::with_capacity(64 * 1024);
    let mut cursor = std::io::Cursor::new(&mut buf);
    if img
        .write_to(&mut cursor, image::ImageOutputFormat::Jpeg(75))
        .is_err()
    {
        warn!("Failed to encode JPEG frame");
        return None;
    }
    Some(buf)
}

/// Process a client input message and forward it to the CEF engine.
#[cfg(feature = "cef-browser")]
fn handle_client_message(text: &str, engine: &Arc<CefBrowserEngine>) {
    let msg: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            debug!("Invalid viewer message: {e}");
            return;
        }
    };

    // Resolve active tab (first tab for now).
    let active_tab = match engine.get_tabs_sync().first().map(|t| t.id) {
        Some(id) => id,
        None => return,
    };

    match msg {
        ClientMessage::MouseMove { x, y } => {
            engine.send_mouse_move(active_tab, x, y);
        }
        ClientMessage::MouseClick { x, y, button } => {
            engine.send_mouse_click(active_tab, x, y, button);
        }
        ClientMessage::MouseWheel {
            x,
            y,
            delta_x,
            delta_y,
        } => {
            engine.send_mouse_wheel(active_tab, x, y, delta_x, delta_y);
        }
        ClientMessage::KeyEvent {
            event_type,
            modifiers,
            windows_key_code,
            character,
        } => {
            engine.send_key_event(active_tab, event_type, modifiers, windows_key_code, character);
        }
        ClientMessage::TypeText { text } => {
            engine.send_type_text(active_tab, &text);
        }
        ClientMessage::Navigate { url } => {
            engine.send_navigate(active_tab, &url);
        }
        ClientMessage::CreateTab { url } => {
            let _ = engine.send_create_tab(&url);
        }
        ClientMessage::CloseTab { tab_id } => {
            if let Ok(uuid) = Uuid::parse_str(&tab_id) {
                engine.send_close_tab(uuid);
            }
        }
        ClientMessage::SetActiveTab { .. } => {
            // Multi-tab switching will be implemented when the client supports it.
            debug!("SetActiveTab not yet implemented");
        }
        ClientMessage::Resize { width, height } => {
            engine.send_resize_viewport(active_tab, width, height);
        }
        ClientMessage::GoBack => {
            engine.send_go_back(active_tab);
        }
        ClientMessage::GoForward => {
            engine.send_go_forward(active_tab);
        }
    }
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
