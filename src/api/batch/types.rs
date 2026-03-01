//! Batch operation type definitions for multi-request execution.
//!
//! Contains all structs and enums used by the batch API: request types,
//! response types, wait conditions, navigation/extraction convenience
//! types, and serialization helpers.

use serde::{Deserialize, Serialize};

// ============================================================================
// Helper Functions
// ============================================================================

/// Serde default function returning `true` for `stop_on_error` fields.
pub(crate) fn default_true() -> bool {
    true
}

// ============================================================================
// Batch Request Types
// ============================================================================

/// A batch request containing multiple operations to execute atomically.
///
/// Operations can run sequentially (default) or in parallel. The
/// `stop_on_error` flag controls whether execution halts on the first
/// failure or continues through all operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRequest {
    /// List of operations to execute
    pub operations: Vec<BatchOperation>,

    /// Execute operations in parallel (default: false = sequential)
    #[serde(default)]
    pub parallel: bool,

    /// Stop on first error (default: true)
    #[serde(default = "default_true")]
    pub stop_on_error: bool,

    /// Timeout for the entire batch in milliseconds
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// A single operation within a batch request.
///
/// Each operation has a unique ID for correlation in results, a command
/// to execute, and optional wait/delay directives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperation {
    /// Operation ID for reference in results
    pub id: String,

    /// The command to execute
    pub command: BatchCommand,

    /// Optional wait condition before executing this operation
    #[serde(default)]
    pub wait_before: Option<WaitCondition>,

    /// Optional delay before executing this operation (milliseconds)
    #[serde(default)]
    pub delay_ms: Option<u64>,
}

/// Commands that can be issued within a batch.
///
/// Each variant maps to an existing browser control operation but is
/// expressed as a self-contained enum for batch serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BatchCommand {
    /// Navigate a tab to a URL
    Navigate {
        url: String,
        #[serde(default)]
        tab_id: Option<String>,
    },

    /// Click on an element identified by CSS selector
    Click {
        selector: String,
        #[serde(default)]
        tab_id: Option<String>,
    },

    /// Type text into an element
    Type {
        selector: String,
        text: String,
        #[serde(default)]
        tab_id: Option<String>,
        #[serde(default)]
        clear_first: Option<bool>,
    },

    /// Capture a screenshot
    Screenshot {
        #[serde(default)]
        tab_id: Option<String>,
        #[serde(default)]
        format: Option<String>,
        #[serde(default)]
        full_page: Option<bool>,
    },

    /// Evaluate JavaScript in the page context
    Evaluate {
        script: String,
        #[serde(default)]
        tab_id: Option<String>,
    },

    /// Scroll the page by delta amounts
    Scroll {
        #[serde(default)]
        delta_x: Option<f64>,
        #[serde(default)]
        delta_y: Option<f64>,
        #[serde(default)]
        tab_id: Option<String>,
    },

    /// Wait for a condition to be met
    Wait {
        condition: WaitCondition,
    },

    /// Extract structured data from the current page
    ExtractStructuredData {
        #[serde(default)]
        tab_id: Option<String>,
    },

    /// Extract text content from the current page
    ExtractContent {
        #[serde(default)]
        tab_id: Option<String>,
    },

    /// Detect forms on the current page
    DetectForms {
        #[serde(default)]
        tab_id: Option<String>,
    },

    /// Open a new browser tab
    NewTab {
        #[serde(default)]
        url: Option<String>,
    },

    /// Close an existing tab
    CloseTab {
        tab_id: String,
    },
}

/// Conditions to wait for before or during batch execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WaitCondition {
    /// Wait for an element matching the CSS selector to appear in the DOM
    Selector {
        selector: String,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },

    /// Wait for a page navigation to complete
    Navigation {
        #[serde(default)]
        timeout_ms: Option<u64>,
    },

    /// Wait for network activity to become idle
    NetworkIdle {
        #[serde(default)]
        timeout_ms: Option<u64>,
    },

    /// Wait for a fixed duration
    Delay {
        ms: u64,
    },

    /// Wait for a JavaScript expression to evaluate to `true`
    Function {
        expression: String,
        #[serde(default)]
        timeout_ms: Option<u64>,
    },
}

// ============================================================================
// Batch Response Types
// ============================================================================

/// Result of executing a complete batch request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResponse {
    /// Whether all operations completed successfully
    pub success: bool,

    /// Per-operation results in execution order
    pub results: Vec<BatchOperationResult>,

    /// Total wall-clock execution time in milliseconds
    pub total_time_ms: u64,

    /// Number of operations that succeeded
    pub succeeded: usize,

    /// Number of operations that failed
    pub failed: usize,
}

/// Result of a single operation within a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationResult {
    /// The operation ID from the request
    pub id: String,

    /// Whether this operation succeeded
    pub success: bool,

    /// Response data (command-specific JSON payload)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Error message if the operation failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Execution time for this operation in milliseconds
    pub duration_ms: u64,
}

// ============================================================================
// Convenience: Batch Navigate & Extract
// ============================================================================

/// Convenience request to navigate to multiple URLs and extract data from each.
///
/// This is a higher-level abstraction over raw batch operations, designed
/// for the common AI-agent pattern of visiting several pages to gather
/// information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchNavigateExtract {
    /// URLs to visit
    pub urls: Vec<String>,

    /// Maximum number of tabs to keep open simultaneously
    #[serde(default)]
    pub parallel_limit: Option<usize>,

    /// What to extract from each page
    pub extract: ExtractOptions,

    /// Milliseconds to wait after each navigation completes
    #[serde(default)]
    pub wait_after_navigate_ms: Option<u64>,
}

/// Configuration for what data to extract from a visited page.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtractOptions {
    /// Capture a screenshot
    #[serde(default)]
    pub screenshot: bool,

    /// Extract raw HTML
    #[serde(default)]
    pub html: bool,

    /// Extract visible text content
    #[serde(default)]
    pub text: bool,

    /// Extract page metadata (title, description, etc.)
    #[serde(default)]
    pub metadata: bool,

    /// Extract structured data (JSON-LD, microdata, etc.)
    #[serde(default)]
    pub structured_data: bool,

    /// Detect and describe forms on the page
    #[serde(default)]
    pub forms: bool,

    /// Extract all links from the page
    #[serde(default)]
    pub links: bool,
}

/// Result of a batch-navigate-and-extract operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchNavigateResult {
    /// Per-URL results
    pub results: Vec<PageResult>,

    /// Total wall-clock time in milliseconds
    pub total_time_ms: u64,
}

/// Extracted data from a single page visit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageResult {
    /// The URL that was visited
    pub url: String,

    /// Whether the page loaded and extraction succeeded
    pub success: bool,

    /// Page title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Base64-encoded screenshot
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,

    /// Raw HTML content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,

    /// Visible text content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    /// Page metadata as JSON
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,

    /// Structured data found on the page
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_data: Option<serde_json::Value>,

    /// Detected forms
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forms: Option<serde_json::Value>,

    /// Links extracted from the page
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<LinkInfo>>,

    /// Error message if extraction failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Time spent on this page in milliseconds
    pub duration_ms: u64,
}

/// Information about a link found on a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkInfo {
    /// The href attribute value
    pub href: String,

    /// The visible link text
    pub text: String,

    /// The rel attribute value, if present
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel: Option<String>,

    /// Whether the link points to an external domain
    pub is_external: bool,
}
