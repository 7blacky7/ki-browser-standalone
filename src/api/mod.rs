//! REST API module for ki-browser-standalone
//!
//! This module provides HTTP and WebSocket APIs for browser control,
//! compatible with the KI-Browser API design.

pub mod ipc;
pub mod routes;
pub mod server;
pub mod websocket;

pub use ipc::{IpcChannel, IpcCommand, IpcMessage, IpcResponse};
pub use routes::create_router;
pub use server::{ApiServer, AppState};
pub use websocket::{BrowserEvent, WebSocketHandler};
