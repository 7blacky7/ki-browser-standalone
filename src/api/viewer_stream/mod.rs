//! Viewer stream module — WebSocket-based JPEG frame streaming with input forwarding.
//!
//! Provides `/ws/viewer` endpoint for remote GUI clients that receive
//! a live video feed of the browser viewport and send input events back.

pub mod handler;
pub mod protocol;

pub use handler::viewer_ws_handler;
