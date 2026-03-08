//! WebSocket connection to the ki-browser server /ws/viewer endpoint.
//!
//! Manages the async WebSocket lifecycle: connecting, receiving encoded frames
//! (JPEG or H.264) and JSON control messages, sending input events back.

use crate::decoder::{self, FrameDecoder};
use crate::protocol::{ClientMessage, ServerMessage, TabInfo};
use futures::{SinkExt, StreamExt};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

/// Shared state updated by the WebSocket receive loop, read by the GUI.
pub struct ViewerState {
    /// Latest decoded frame as RGBA pixels.
    pub frame_rgba: Mutex<Option<FrameData>>,
    /// Current tab list from the server.
    pub tabs: Mutex<Vec<TabInfo>>,
    /// Currently active tab ID.
    pub active_tab: Mutex<Option<String>>,
    /// Connection status.
    pub connected: Mutex<bool>,
    /// Last error message from server.
    pub last_error: Mutex<Option<String>>,
}

/// Decoded frame data ready for GPU upload.
pub struct FrameData {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl ViewerState {
    pub fn new() -> Self {
        Self {
            frame_rgba: Mutex::new(None),
            tabs: Mutex::new(Vec::new()),
            active_tab: Mutex::new(None),
            connected: Mutex::new(false),
            last_error: Mutex::new(None),
        }
    }
}

/// Runs the WebSocket connection in a background tokio task.
/// Returns a sender for outgoing client messages.
pub fn spawn_connection(
    url: String,
    state: Arc<ViewerState>,
    ctx: egui::Context,
) -> mpsc::UnboundedSender<ClientMessage> {
    let (input_tx, input_rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        if let Err(e) = run_connection(&url, state.clone(), input_rx, ctx.clone()).await {
            error!("WebSocket connection error: {e}");
            *state.last_error.lock() = Some(format!("Connection failed: {e}"));
            *state.connected.lock() = false;
            ctx.request_repaint();
        }
    });

    input_tx
}

/// Main connection loop: connect, receive frames + messages, send input.
async fn run_connection(
    url: &str,
    state: Arc<ViewerState>,
    mut input_rx: mpsc::UnboundedReceiver<ClientMessage>,
    ctx: egui::Context,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Connecting to {url}");
    let (ws_stream, _) = tokio_tungstenite::connect_async(url).await?;
    info!("Connected to server");
    *state.connected.lock() = true;
    ctx.request_repaint();

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Receive loop: handle incoming frames and control messages.
    let recv_state = state.clone();
    let recv_ctx = ctx.clone();
    let recv_task = tokio::spawn(async move {
        let mut jpeg_dec = decoder::JpegDecoder;
        let mut h264_dec: Option<Box<dyn FrameDecoder>> = None;

        while let Some(msg_result) = ws_receiver.next().await {
            match msg_result {
                Ok(Message::Binary(data)) => {
                    if data.is_empty() {
                        continue;
                    }
                    match data[0] {
                        // Prefixed JPEG frame.
                        0x00 => {
                            if let Some(frame) = jpeg_dec.decode(&data[1..]) {
                                *recv_state.frame_rgba.lock() = Some(FrameData {
                                    rgba: frame.rgba,
                                    width: frame.width,
                                    height: frame.height,
                                });
                                recv_ctx.request_repaint();
                            }
                        }
                        // H.264 codec config (SPS/PPS).
                        0x01 => {
                            debug!("H.264 config: {} bytes", data.len() - 1);
                            let dec = h264_dec.get_or_insert_with(decoder::create_decoder);
                            dec.set_config(&data[1..]);
                        }
                        // H.264 frame data.
                        0x02 => {
                            let dec = h264_dec.get_or_insert_with(decoder::create_decoder);
                            if let Some(frame) = dec.decode(&data[1..]) {
                                *recv_state.frame_rgba.lock() = Some(FrameData {
                                    rgba: frame.rgba,
                                    width: frame.width,
                                    height: frame.height,
                                });
                                recv_ctx.request_repaint();
                            }
                        }
                        // Legacy: no prefix, raw JPEG (0xFF = JPEG SOI marker).
                        0xFF => {
                            if let Some(frame) = jpeg_dec.decode(&data) {
                                *recv_state.frame_rgba.lock() = Some(FrameData {
                                    rgba: frame.rgba,
                                    width: frame.width,
                                    height: frame.height,
                                });
                                recv_ctx.request_repaint();
                            }
                        }
                        prefix => {
                            debug!("Unknown binary prefix: 0x{prefix:02X}");
                        }
                    }
                }
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<ServerMessage>(&text) {
                        Ok(ServerMessage::Connected { active_tab, tabs }) => {
                            info!("Server connected: {} tabs", tabs.len());
                            *recv_state.tabs.lock() = tabs;
                            *recv_state.active_tab.lock() = active_tab;
                            recv_ctx.request_repaint();
                        }
                        Ok(ServerMessage::TabUpdate { active_tab, tabs }) => {
                            debug!("Tab update: {} tabs", tabs.len());
                            *recv_state.tabs.lock() = tabs;
                            *recv_state.active_tab.lock() = active_tab;
                            recv_ctx.request_repaint();
                        }
                        Ok(ServerMessage::Error { message }) => {
                            error!("Server error: {message}");
                            *recv_state.last_error.lock() = Some(message);
                            recv_ctx.request_repaint();
                        }
                        Err(e) => debug!("Unknown server message: {e}"),
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Server closed connection");
                    break;
                }
                Err(e) => {
                    error!("WebSocket receive error: {e}");
                    break;
                }
                _ => {}
            }
        }
        *recv_state.connected.lock() = false;
        recv_ctx.request_repaint();
    });

    // Send loop: forward client input messages to server.
    let send_task = tokio::spawn(async move {
        while let Some(msg) = input_rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(j) => j,
                Err(e) => {
                    warn!("Failed to serialize client message: {e}");
                    continue;
                }
            };
            if ws_sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = recv_task => {},
        _ = send_task => {},
    }

    *state.connected.lock() = false;
    ctx.request_repaint();
    Ok(())
}
