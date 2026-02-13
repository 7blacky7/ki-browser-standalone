//! REST API module for ki-browser-standalone
//!
//! This module provides HTTP and WebSocket APIs for browser control,
//! compatible with the KI-Browser API design.

pub mod browser_handler;
pub mod ipc;
pub mod routes;
pub mod server;
pub mod websocket;

pub use browser_handler::{BrowserCommandHandler, BrowserEngineWrapper};
pub use ipc::{IpcChannel, IpcCommand, IpcMessage, IpcProcessor, IpcResponse};
pub use routes::create_router;
pub use server::{ApiServer, AppState};
pub use websocket::{BrowserEvent, WebSocketHandler};
