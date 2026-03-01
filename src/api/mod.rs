//! REST API module for ki-browser-standalone
//!
//! This module provides HTTP and WebSocket APIs for browser control,
//! compatible with the KI-Browser API design.

pub mod agent_registry;
pub mod agent_routes;
pub mod batch;
pub mod batch_routes;
pub mod browser_handler;
pub mod extraction_routes;
pub mod ipc;
pub mod routes;
pub mod server;
pub mod session;
pub mod vision_routes;
pub mod websocket;

pub use agent_registry::{AgentInfo, AgentRegistry};
pub use agent_routes::agent_routes;
pub use batch::{
    BatchCommand, BatchNavigateExtract, BatchNavigateResult, BatchOperation, BatchRequest,
    BatchResponse, ExtractOptions, LinkInfo, PageResult, WaitCondition,
};
pub use browser_handler::{BrowserCommandHandler, BrowserEngineWrapper};
pub use ipc::{IpcChannel, IpcCommand, IpcMessage, IpcProcessor, IpcResponse};
pub use batch_routes::batch_session_routes;
pub use extraction_routes::extraction_routes;
pub use routes::create_router;
pub use server::{ApiServer, AppState};
pub use session::{
    CookieInfo, HistoryEntry, Session, SessionManager, SessionSnapshot, TabSnapshot,
};
pub use vision_routes::vision_routes;
pub use websocket::{BrowserEvent, WebSocketHandler};
