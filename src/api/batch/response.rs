//! Batch response construction and result tracking.
//!
//! Provides methods on `BatchResponse` to incrementally record
//! per-operation successes and failures, then finalize with total
//! timing information.

use super::types::{BatchOperationResult, BatchResponse};

impl BatchResponse {
    /// Create a new empty batch response.
    pub fn new() -> Self {
        Self {
            success: true,
            results: Vec::new(),
            total_time_ms: 0,
            succeeded: 0,
            failed: 0,
        }
    }

    /// Record a successful operation result.
    pub fn add_success(&mut self, id: String, data: Option<serde_json::Value>, duration_ms: u64) {
        self.results.push(BatchOperationResult {
            id,
            success: true,
            data,
            error: None,
            duration_ms,
        });
        self.succeeded += 1;
    }

    /// Record a failed operation result.
    pub fn add_failure(&mut self, id: String, error: String, duration_ms: u64) {
        self.results.push(BatchOperationResult {
            id,
            success: false,
            data: None,
            error: Some(error),
            duration_ms,
        });
        self.failed += 1;
        self.success = false;
    }

    /// Finalize the response with total timing.
    pub fn finalize(&mut self, total_time_ms: u64) {
        self.total_time_ms = total_time_ms;
    }
}

impl Default for BatchResponse {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::super::types::*;

    #[test]
    fn test_batch_response_tracking() {
        let mut resp = BatchResponse::new();
        assert!(resp.success);

        resp.add_success(
            "op1".to_string(),
            Some(serde_json::json!({"ok": true})),
            100,
        );
        assert_eq!(resp.succeeded, 1);
        assert_eq!(resp.failed, 0);
        assert!(resp.success);

        resp.add_failure("op2".to_string(), "Element not found".to_string(), 50);
        assert_eq!(resp.succeeded, 1);
        assert_eq!(resp.failed, 1);
        assert!(!resp.success);

        resp.finalize(200);
        assert_eq!(resp.total_time_ms, 200);
        assert_eq!(resp.results.len(), 2);
    }
}
