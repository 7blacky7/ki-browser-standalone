//! Console-log capture endpoints.
//!
//! Provides a ring-buffer that stores browser console messages (log, warn, error, etc.)
//! and REST endpoints to retrieve and clear them.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::api::routes::ApiResponse;
use crate::api::server::AppState;

// ============================================================================
// Types
// ============================================================================

/// A single captured console log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsoleLogEntry {
    pub tab_id: String,
    pub level: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub timestamp: String,
}

/// Ring-buffer for console log entries with configurable capacity.
#[derive(Debug)]
pub struct ConsoleLogBuffer {
    entries: VecDeque<ConsoleLogEntry>,
    capacity: usize,
}

impl ConsoleLogBuffer {
    /// Create a new buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push a new entry, dropping the oldest if at capacity.
    pub fn push(&mut self, entry: ConsoleLogEntry) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Get entries, optionally filtered by tab_id and level.
    pub fn get_entries(
        &self,
        tab_id: Option<&str>,
        level: Option<&str>,
        limit: usize,
    ) -> Vec<ConsoleLogEntry> {
        self.entries
            .iter()
            .filter(|e| tab_id.map_or(true, |id| e.tab_id == id))
            .filter(|e| level.map_or(true, |l| e.level == l))
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Clear entries, optionally only for a specific tab.
    pub fn clear(&mut self, tab_id: Option<&str>) {
        if let Some(id) = tab_id {
            self.entries.retain(|e| e.tab_id != id);
        } else {
            self.entries.clear();
        }
    }

    /// Total entries in the buffer.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Buffer capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl Default for ConsoleLogBuffer {
    fn default() -> Self {
        Self::new(1000)
    }
}

/// Create a ConsoleLogEntry from raw CEF callback data.
pub fn create_log_entry(
    tab_id: &str,
    level: &str,
    message: &str,
    source: Option<String>,
    line: Option<u32>,
) -> ConsoleLogEntry {
    ConsoleLogEntry {
        tab_id: tab_id.to_string(),
        level: level.to_string(),
        message: message.to_string(),
        source,
        line,
        timestamp: Utc::now().to_rfc3339(),
    }
}

// ============================================================================
// Query Parameters
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct ConsoleQuery {
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

// ============================================================================
// Responses
// ============================================================================

#[derive(Debug, Serialize)]
pub struct ConsoleLogsResponse {
    pub entries: Vec<ConsoleLogEntry>,
    pub total: usize,
    pub buffer_capacity: usize,
}

#[derive(Debug, Serialize)]
pub struct ConsoleClearResponse {
    pub cleared: bool,
}

// ============================================================================
// Handlers
// ============================================================================

async fn get_console_logs(
    State(state): State<AppState>,
    Query(query): Query<ConsoleQuery>,
) -> impl IntoResponse {
    let buffer = state.console_log_buffer.read().await;
    let entries = buffer.get_entries(
        query.tab_id.as_deref(),
        query.level.as_deref(),
        query.limit,
    );
    let total = buffer.len();
    let capacity = buffer.capacity();

    Json(ApiResponse::success(ConsoleLogsResponse {
        entries,
        total,
        buffer_capacity: capacity,
    }))
}

async fn clear_console_logs(
    State(state): State<AppState>,
    Query(query): Query<super::types::TabQuery>,
) -> impl IntoResponse {
    let mut buffer = state.console_log_buffer.write().await;
    buffer.clear(query.tab_id.as_deref());

    Json(ApiResponse::success(ConsoleClearResponse { cleared: true }))
}

// ============================================================================
// Router
// ============================================================================

pub fn console_routes() -> Router<AppState> {
    Router::new()
        .route("/debug/console", get(get_console_logs).delete(clear_console_logs))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_console_log_buffer_push_and_get() {
        let mut buffer = ConsoleLogBuffer::new(3);
        buffer.push(create_log_entry("tab1", "log", "hello", None, None));
        buffer.push(create_log_entry("tab1", "error", "oops", Some("test.js".into()), Some(42)));
        buffer.push(create_log_entry("tab2", "warn", "caution", None, None));

        let all = buffer.get_entries(None, None, 100);
        assert_eq!(all.len(), 3);

        let tab1 = buffer.get_entries(Some("tab1"), None, 100);
        assert_eq!(tab1.len(), 2);

        let errors = buffer.get_entries(None, Some("error"), 100);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, "oops");
    }

    #[test]
    fn test_console_log_buffer_capacity() {
        let mut buffer = ConsoleLogBuffer::new(2);
        buffer.push(create_log_entry("t", "log", "first", None, None));
        buffer.push(create_log_entry("t", "log", "second", None, None));
        buffer.push(create_log_entry("t", "log", "third", None, None));

        assert_eq!(buffer.len(), 2);
        let entries = buffer.get_entries(None, None, 100);
        assert_eq!(entries[0].message, "second");
        assert_eq!(entries[1].message, "third");
    }

    #[test]
    fn test_console_log_buffer_clear() {
        let mut buffer = ConsoleLogBuffer::new(10);
        buffer.push(create_log_entry("tab1", "log", "a", None, None));
        buffer.push(create_log_entry("tab2", "log", "b", None, None));

        buffer.clear(Some("tab1"));
        assert_eq!(buffer.len(), 1);

        buffer.clear(None);
        assert_eq!(buffer.len(), 0);
    }

    #[test]
    fn test_console_log_entry_serialization() {
        let entry = create_log_entry("tab1", "error", "test", Some("file.js".into()), Some(10));
        let json = serde_json::to_string(&entry).expect("serialize");
        assert!(json.contains("\"level\":\"error\""));
        assert!(json.contains("\"line\":10"));
    }

    #[test]
    fn test_console_logs_response_serialization() {
        let resp = ConsoleLogsResponse {
            entries: vec![],
            total: 0,
            buffer_capacity: 1000,
        };
        let json = serde_json::to_string(&resp).expect("serialize");
        assert!(json.contains("\"buffer_capacity\":1000"));
    }

    #[test]
    fn test_get_entries_limit() {
        let mut buffer = ConsoleLogBuffer::new(10);
        for i in 0..5 {
            buffer.push(create_log_entry("t", "log", &format!("msg{}", i), None, None));
        }
        let entries = buffer.get_entries(None, None, 2);
        assert_eq!(entries.len(), 2);
        // Should be the last 2 entries
        assert_eq!(entries[0].message, "msg3");
        assert_eq!(entries[1].message, "msg4");
    }
}
