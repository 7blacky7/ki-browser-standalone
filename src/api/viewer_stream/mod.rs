//! Viewer stream module — WebSocket-based frame streaming with input forwarding.
//!
//! Provides `/ws/viewer` endpoint for remote GUI clients that receive
//! a live video feed of the browser viewport and send input events back.
//! Supports JPEG (default) and H.264 NVENC hardware encoding (h264 feature).

pub mod encoder;
pub mod handler;
pub mod protocol;

pub use handler::viewer_ws_handler;
