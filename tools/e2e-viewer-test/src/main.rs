//! E2E test tool for the ki-browser WebSocket viewer stream endpoint.
//!
//! Connects to /ws/viewer, receives frames (JPEG/H.264), sends input events,
//! and produces a PASS/FAIL report based on connection success and frame reception.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::Parser;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

/// E2E test CLI for the ki-browser WebSocket viewer stream.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// WebSocket URL of the viewer endpoint.
    #[arg(long, default_value = "ws://127.0.0.1:3000/ws/viewer")]
    url: String,

    /// Total test duration in seconds before disconnecting.
    #[arg(long, default_value_t = 10)]
    timeout: u64,

    /// Optional path to save the first received JPEG frame as PNG.
    #[arg(long)]
    save_frame: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Protocol types – Server -> Client (JSON text messages)
// ---------------------------------------------------------------------------

/// Tab metadata sent by the server inside connected/tab_update messages.
#[derive(Debug, Clone, Deserialize)]
struct TabInfo {
    id: String,
    url: String,
    title: String,
}

/// JSON text messages received from the server.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ServerMessage {
    /// Initial connection acknowledgement with current tab state.
    #[serde(rename = "connected")]
    Connected {
        active_tab: Option<String>,
        tabs: Vec<TabInfo>,
    },
    /// Notification that the tab list or active tab changed.
    #[serde(rename = "tab_update")]
    TabUpdate {
        active_tab: Option<String>,
        tabs: Vec<TabInfo>,
    },
    /// Server-side error message.
    #[serde(rename = "error")]
    Error { message: String },
}

// ---------------------------------------------------------------------------
// Protocol types – Client -> Server (JSON text messages)
// ---------------------------------------------------------------------------

/// JSON text messages the client can send to the server.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ClientMessage {
    /// Simulate a mouse movement to the given coordinates.
    #[serde(rename = "mouse_move")]
    MouseMove { x: i32, y: i32 },
    /// Request navigation to a URL.
    #[serde(rename = "navigate")]
    Navigate { url: String },
}

// ---------------------------------------------------------------------------
// Binary frame prefix constants
// ---------------------------------------------------------------------------

/// JPEG frame (prefix byte 0x00, followed by JPEG data).
const PREFIX_JPEG: u8 = 0x00;
/// H.264 SPS/PPS configuration data (prefix 0x01).
const PREFIX_H264_CONFIG: u8 = 0x01;
/// H.264 NAL unit frame data (prefix 0x02).
const PREFIX_H264_NAL: u8 = 0x02;
/// Legacy raw JPEG – entire binary message is the JPEG (starts with FF D8).
const PREFIX_LEGACY_JPEG: u8 = 0xFF;

// ---------------------------------------------------------------------------
// Test statistics
// ---------------------------------------------------------------------------

/// Accumulated statistics collected during the test run.
#[derive(Debug, Default)]
struct TestStats {
    connected: bool,
    tabs_received: usize,
    active_tab: Option<String>,
    jpeg_frames: u64,
    h264_config_frames: u64,
    h264_nal_frames: u64,
    legacy_jpeg_frames: u64,
    tab_updates: usize,
    mouse_move_sent: bool,
    navigate_sent: bool,
    first_frame_saved: Option<(PathBuf, u32, u32)>,
    first_frame_time: Option<Instant>,
    last_frame_time: Option<Instant>,
}

impl TestStats {
    /// Total number of received binary frames across all types.
    fn total_frames(&self) -> u64 {
        self.jpeg_frames + self.h264_nal_frames + self.legacy_jpeg_frames
    }

    /// Approximate average FPS based on first/last frame timestamps and total frame count.
    fn average_fps(&self) -> f64 {
        match (self.first_frame_time, self.last_frame_time) {
            (Some(first), Some(last)) if last > first && self.total_frames() > 1 => {
                let elapsed = last.duration_since(first).as_secs_f64();
                if elapsed > 0.0 {
                    (self.total_frames() as f64 - 1.0) / elapsed
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }

    /// Whether the test passed: connected successfully and received at least one frame.
    fn passed(&self) -> bool {
        self.connected && self.total_frames() > 0
    }

    /// Print the final human-readable report to stdout.
    fn print_report(&self) {
        println!();
        println!("=== E2E Viewer Test Results ===");
        println!(
            "Connected:            {}",
            if self.connected { "yes" } else { "no" }
        );
        println!("Tabs received:        {}", self.tabs_received);
        println!(
            "Active tab:           {}",
            self.active_tab.as_deref().unwrap_or("<none>")
        );
        println!(
            "Frames received:      {} (JPEG: {}, H264: {}, Config: {}, Legacy: {})",
            self.total_frames() + self.h264_config_frames,
            self.jpeg_frames,
            self.h264_nal_frames,
            self.h264_config_frames,
            self.legacy_jpeg_frames,
        );
        println!("Average FPS:          {:.1}", self.average_fps());
        println!("Tab updates received: {}", self.tab_updates);

        let mut inputs = Vec::new();
        if self.mouse_move_sent {
            inputs.push("mouse_move");
        }
        if self.navigate_sent {
            inputs.push("navigate");
        }
        println!(
            "Input sent:           {}",
            if inputs.is_empty() {
                "none".to_string()
            } else {
                inputs.join(", ")
            }
        );

        match &self.first_frame_saved {
            Some((path, w, h)) => {
                println!(
                    "First frame saved:    {} ({}x{})",
                    path.display(),
                    w,
                    h
                );
            }
            None => {
                println!("First frame saved:    <not saved>");
            }
        }

        let result = if self.passed() { "PASS" } else { "FAIL" };
        println!("Result:               {}", result);
        println!();
    }
}

// ---------------------------------------------------------------------------
// Frame handling helpers
// ---------------------------------------------------------------------------

/// Try to decode JPEG bytes and optionally save as PNG if `save_path` is provided
/// and no frame has been saved yet.
fn try_save_jpeg_frame(
    jpeg_data: &[u8],
    save_path: &Option<PathBuf>,
    stats: &mut TestStats,
) {
    if save_path.is_none() || stats.first_frame_saved.is_some() {
        return;
    }
    let path = save_path.as_ref().unwrap();

    match image::load_from_memory_with_format(jpeg_data, image::ImageFormat::Jpeg) {
        Ok(img) => {
            let w = img.width();
            let h = img.height();
            match img.save(path) {
                Ok(()) => {
                    info!(width = w, height = h, path = %path.display(), "First JPEG frame saved as PNG");
                    stats.first_frame_saved = Some((path.clone(), w, h));
                }
                Err(e) => {
                    warn!("Failed to save frame as PNG: {}", e);
                }
            }
        }
        Err(e) => {
            warn!("Failed to decode JPEG frame: {}", e);
        }
    }
}

/// Classify and count a binary WebSocket message based on its prefix byte.
fn handle_binary_frame(data: &[u8], save_path: &Option<PathBuf>, stats: &mut TestStats) {
    if data.is_empty() {
        warn!("Received empty binary message");
        return;
    }

    let now = Instant::now();
    if stats.first_frame_time.is_none() {
        stats.first_frame_time = Some(now);
    }
    stats.last_frame_time = Some(now);

    let prefix = data[0];
    match prefix {
        PREFIX_JPEG => {
            stats.jpeg_frames += 1;
            if data.len() > 1 {
                try_save_jpeg_frame(&data[1..], save_path, stats);
            }
        }
        PREFIX_H264_CONFIG => {
            stats.h264_config_frames += 1;
        }
        PREFIX_H264_NAL => {
            stats.h264_nal_frames += 1;
        }
        PREFIX_LEGACY_JPEG if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 => {
            // Legacy JPEG: the entire message is JPEG data (starts with FF D8 magic).
            stats.legacy_jpeg_frames += 1;
            try_save_jpeg_frame(data, save_path, stats);
        }
        _ => {
            // Unknown prefix – could still be a legacy JPEG if it starts with FF D8.
            if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
                stats.legacy_jpeg_frames += 1;
                try_save_jpeg_frame(data, save_path, stats);
            } else {
                warn!(prefix = prefix, len = data.len(), "Unknown binary prefix");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    let args = Args::parse();
    let test_duration = Duration::from_secs(args.timeout);

    info!(url = %args.url, timeout_secs = args.timeout, "Starting E2E viewer test");

    let mut stats = TestStats::default();

    // -----------------------------------------------------------------------
    // 1. Connect to the WebSocket endpoint
    // -----------------------------------------------------------------------
    let ws_stream = match timeout(Duration::from_secs(5), tokio_tungstenite::connect_async(&args.url)).await {
        Ok(Ok((stream, _response))) => {
            info!("WebSocket connection established");
            stream
        }
        Ok(Err(e)) => {
            error!("WebSocket connection failed: {}", e);
            stats.print_report();
            std::process::exit(1);
        }
        Err(_) => {
            error!("WebSocket connection timed out (5s)");
            stats.print_report();
            std::process::exit(1);
        }
    };

    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // -----------------------------------------------------------------------
    // 2–7. Run the test loop with scheduled actions
    // -----------------------------------------------------------------------
    let start = Instant::now();

    // Schedule: send mouse_move at 2s, navigate at 3s
    let mut mouse_move_sent = false;
    let mut navigate_sent = false;

    loop {
        let elapsed = start.elapsed();
        if elapsed >= test_duration {
            info!("Timeout reached, closing connection");
            break;
        }

        let remaining = test_duration - elapsed;

        // Determine next scheduled action deadline
        let next_action_deadline = if !mouse_move_sent && elapsed < Duration::from_secs(2) {
            Some(Duration::from_secs(2) - elapsed)
        } else if !navigate_sent && elapsed < Duration::from_secs(3) {
            Some(Duration::from_secs(3) - elapsed)
        } else {
            None
        };

        let wait_time = match next_action_deadline {
            Some(d) => d.min(remaining),
            None => remaining,
        };

        // Wait for either a message or the next action deadline
        match timeout(wait_time, ws_stream.next()).await {
            Ok(Some(Ok(msg))) => {
                match msg {
                    Message::Text(text) => {
                        match serde_json::from_str::<ServerMessage>(&text) {
                            Ok(ServerMessage::Connected { active_tab, tabs }) => {
                                info!(
                                    active_tab = ?active_tab,
                                    num_tabs = tabs.len(),
                                    "Received 'connected' message"
                                );
                                stats.connected = true;
                                stats.tabs_received = tabs.len();
                                stats.active_tab = active_tab;
                                for tab in &tabs {
                                    info!(id = %tab.id, url = %tab.url, title = %tab.title, "  Tab");
                                }
                            }
                            Ok(ServerMessage::TabUpdate { active_tab, tabs }) => {
                                info!(
                                    active_tab = ?active_tab,
                                    num_tabs = tabs.len(),
                                    "Received tab_update"
                                );
                                stats.tab_updates += 1;
                                stats.tabs_received = tabs.len();
                                stats.active_tab = active_tab;
                            }
                            Ok(ServerMessage::Error { message }) => {
                                warn!(message = %message, "Server error");
                            }
                            Err(e) => {
                                warn!(raw = %text, err = %e, "Unrecognized text message");
                            }
                        }
                    }
                    Message::Binary(data) => {
                        handle_binary_frame(&data, &args.save_frame, &mut stats);
                    }
                    Message::Close(_) => {
                        info!("Server closed the connection");
                        break;
                    }
                    Message::Ping(payload) => {
                        let _ = ws_sink.send(Message::Pong(payload)).await;
                    }
                    _ => {}
                }
            }
            Ok(Some(Err(e))) => {
                error!("WebSocket error: {}", e);
                break;
            }
            Ok(None) => {
                info!("WebSocket stream ended");
                break;
            }
            Err(_) => {
                // Timeout reached – time to check scheduled actions
            }
        }

        // Execute scheduled actions based on elapsed time
        let elapsed = start.elapsed();

        if !mouse_move_sent && elapsed >= Duration::from_secs(2) {
            let msg = ClientMessage::MouseMove { x: 500, y: 300 };
            let json = serde_json::to_string(&msg).expect("serialize mouse_move");
            match ws_sink.send(Message::Text(json.into())).await {
                Ok(()) => {
                    info!("Sent mouse_move(500, 300)");
                    mouse_move_sent = true;
                    stats.mouse_move_sent = true;
                }
                Err(e) => {
                    warn!("Failed to send mouse_move: {}", e);
                }
            }
        }

        if !navigate_sent && elapsed >= Duration::from_secs(3) {
            let msg = ClientMessage::Navigate {
                url: "https://example.com".to_string(),
            };
            let json = serde_json::to_string(&msg).expect("serialize navigate");
            match ws_sink.send(Message::Text(json.into())).await {
                Ok(()) => {
                    info!("Sent navigate(https://example.com)");
                    navigate_sent = true;
                    stats.navigate_sent = true;
                }
                Err(e) => {
                    warn!("Failed to send navigate: {}", e);
                }
            }
        }
    }

    // Gracefully close
    let _ = ws_sink.close().await;

    // -----------------------------------------------------------------------
    // 8. Print the report
    // -----------------------------------------------------------------------
    stats.print_report();

    if !stats.passed() {
        std::process::exit(1);
    }
}
