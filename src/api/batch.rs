//! Batch operations for atomic multi-request execution
//!
//! Enables AI agents to execute multiple browser commands as a single
//! atomic unit, with support for sequential and parallel execution,
//! error handling policies, and convenience methods for common
//! multi-step workflows.

use serde::{Deserialize, Serialize};

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

fn default_true() -> bool {
    true
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

// ============================================================================
// Batch Execution Logic
// ============================================================================

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

impl BatchRequest {
    /// Validate the batch request before execution.
    ///
    /// Returns `Ok(())` if the request is valid, or an error string
    /// describing the problem.
    pub fn validate(&self) -> Result<(), String> {
        if self.operations.is_empty() {
            return Err("Batch request must contain at least one operation".to_string());
        }

        // Check for duplicate operation IDs
        let mut seen_ids = std::collections::HashSet::new();
        for op in &self.operations {
            if !seen_ids.insert(&op.id) {
                return Err(format!("Duplicate operation ID: {}", op.id));
            }
        }

        Ok(())
    }

    /// Convert batch operations into a sequence of IPC commands.
    ///
    /// Returns a list of `(operation_id, IpcCommand)` tuples suitable
    /// for dispatching through the IPC channel.
    pub fn to_ipc_commands(&self, default_tab_id: Option<&str>) -> Vec<(String, crate::api::ipc::IpcCommand)> {
        self.operations
            .iter()
            .filter_map(|op| {
                let tab = |explicit: &Option<String>| -> String {
                    explicit
                        .clone()
                        .or_else(|| default_tab_id.map(String::from))
                        .unwrap_or_default()
                };

                let cmd = match &op.command {
                    BatchCommand::Navigate { url, tab_id } => {
                        crate::api::ipc::IpcCommand::Navigate {
                            tab_id: tab(tab_id),
                            url: url.clone(),
                        }
                    }
                    BatchCommand::Click { selector, tab_id } => {
                        crate::api::ipc::IpcCommand::ClickElement {
                            tab_id: tab(tab_id),
                            selector: selector.clone(),
                            button: "left".to_string(),
                            modifiers: None,
                        }
                    }
                    BatchCommand::Type { selector, text, tab_id, clear_first } => {
                        crate::api::ipc::IpcCommand::TypeText {
                            tab_id: tab(tab_id),
                            text: text.clone(),
                            selector: Some(selector.clone()),
                            clear_first: clear_first.unwrap_or(false),
                        }
                    }
                    BatchCommand::Screenshot { tab_id, format, full_page } => {
                        crate::api::ipc::IpcCommand::CaptureScreenshot {
                            tab_id: tab(tab_id),
                            format: format.clone().unwrap_or_else(|| "png".to_string()),
                            quality: None,
                            full_page: full_page.unwrap_or(false),
                            selector: None,
                        }
                    }
                    BatchCommand::Evaluate { script, tab_id } => {
                        crate::api::ipc::IpcCommand::EvaluateScript {
                            tab_id: tab(tab_id),
                            script: script.clone(),
                            await_promise: true,
                        }
                    }
                    BatchCommand::Scroll { delta_x, delta_y, tab_id } => {
                        crate::api::ipc::IpcCommand::Scroll {
                            tab_id: tab(tab_id),
                            x: None,
                            y: None,
                            delta_x: delta_x.map(|v| v as i32),
                            delta_y: delta_y.map(|v| v as i32),
                            selector: None,
                            behavior: None,
                        }
                    }
                    BatchCommand::Wait { .. } => {
                        // Wait operations are handled by the executor, not IPC
                        return None;
                    }
                    BatchCommand::ExtractStructuredData { tab_id } => {
                        crate::api::ipc::IpcCommand::EvaluateScript {
                            tab_id: tab(tab_id),
                            script: extract_structured_data_script().to_string(),
                            await_promise: true,
                        }
                    }
                    BatchCommand::ExtractContent { tab_id } => {
                        crate::api::ipc::IpcCommand::EvaluateScript {
                            tab_id: tab(tab_id),
                            script: extract_content_script().to_string(),
                            await_promise: true,
                        }
                    }
                    BatchCommand::DetectForms { tab_id } => {
                        crate::api::ipc::IpcCommand::EvaluateScript {
                            tab_id: tab(tab_id),
                            script: detect_forms_script().to_string(),
                            await_promise: true,
                        }
                    }
                    BatchCommand::NewTab { url } => {
                        crate::api::ipc::IpcCommand::CreateTab {
                            url: url.clone().unwrap_or_else(|| "about:blank".to_string()),
                            active: true,
                        }
                    }
                    BatchCommand::CloseTab { tab_id } => {
                        crate::api::ipc::IpcCommand::CloseTab {
                            tab_id: tab_id.clone(),
                        }
                    }
                };

                Some((op.id.clone(), cmd))
            })
            .collect()
    }
}

/// Generate a wait condition as a JavaScript polling script.
///
/// Returns a JS expression string that the browser can evaluate. The
/// expression resolves to `true` when the condition is met.
impl WaitCondition {
    /// Get the effective timeout for this wait condition (in milliseconds).
    pub fn timeout_ms(&self) -> u64 {
        match self {
            WaitCondition::Selector { timeout_ms, .. } => timeout_ms.unwrap_or(10_000),
            WaitCondition::Navigation { timeout_ms } => timeout_ms.unwrap_or(30_000),
            WaitCondition::NetworkIdle { timeout_ms } => timeout_ms.unwrap_or(10_000),
            WaitCondition::Delay { ms } => *ms,
            WaitCondition::Function { timeout_ms, .. } => timeout_ms.unwrap_or(10_000),
        }
    }

    /// Convert this wait condition into a JavaScript expression that polls
    /// until the condition is met or the timeout expires.
    pub fn to_js_expression(&self) -> String {
        match self {
            WaitCondition::Selector { selector, timeout_ms } => {
                let timeout = timeout_ms.unwrap_or(10_000);
                let escaped = selector.replace('\\', "\\\\").replace('\'', "\\'");
                format!(
                    r#"new Promise((resolve, reject) => {{
    const timeout = {timeout};
    const start = Date.now();
    const check = () => {{
        if (document.querySelector('{escaped}')) {{
            resolve(true);
        }} else if (Date.now() - start > timeout) {{
            reject(new Error('Timeout waiting for selector: {escaped}'));
        }} else {{
            requestAnimationFrame(check);
        }}
    }};
    check();
}})"#
                )
            }
            WaitCondition::Navigation { timeout_ms } => {
                let timeout = timeout_ms.unwrap_or(30_000);
                format!(
                    r#"new Promise((resolve, reject) => {{
    const timeout = {timeout};
    const timer = setTimeout(() => {{
        reject(new Error('Navigation timeout'));
    }}, timeout);
    if (document.readyState === 'complete') {{
        clearTimeout(timer);
        resolve(true);
    }} else {{
        window.addEventListener('load', () => {{
            clearTimeout(timer);
            resolve(true);
        }}, {{ once: true }});
    }}
}})"#
                )
            }
            WaitCondition::NetworkIdle { timeout_ms } => {
                let timeout = timeout_ms.unwrap_or(10_000);
                format!(
                    r#"new Promise((resolve, reject) => {{
    const timeout = {timeout};
    const idleThreshold = 500;
    let lastActivity = Date.now();
    const start = Date.now();
    const origFetch = window.fetch;
    let pending = 0;
    window.fetch = function(...args) {{
        pending++;
        lastActivity = Date.now();
        return origFetch.apply(this, args).finally(() => {{
            pending--;
            lastActivity = Date.now();
        }});
    }};
    const origXhrOpen = XMLHttpRequest.prototype.open;
    const origXhrSend = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.open = function(...args) {{
        this.__netIdle = true;
        return origXhrOpen.apply(this, args);
    }};
    XMLHttpRequest.prototype.send = function(...args) {{
        if (this.__netIdle) {{
            pending++;
            lastActivity = Date.now();
            this.addEventListener('loadend', () => {{
                pending--;
                lastActivity = Date.now();
            }}, {{ once: true }});
        }}
        return origXhrSend.apply(this, args);
    }};
    const check = () => {{
        if (Date.now() - start > timeout) {{
            window.fetch = origFetch;
            XMLHttpRequest.prototype.open = origXhrOpen;
            XMLHttpRequest.prototype.send = origXhrSend;
            reject(new Error('Network idle timeout'));
        }} else if (pending === 0 && Date.now() - lastActivity > idleThreshold) {{
            window.fetch = origFetch;
            XMLHttpRequest.prototype.open = origXhrOpen;
            XMLHttpRequest.prototype.send = origXhrSend;
            resolve(true);
        }} else {{
            setTimeout(check, 100);
        }}
    }};
    setTimeout(check, idleThreshold);
}})"#
                )
            }
            WaitCondition::Delay { ms } => {
                format!(
                    "new Promise(resolve => setTimeout(resolve, {ms}))"
                )
            }
            WaitCondition::Function { expression, timeout_ms } => {
                let timeout = timeout_ms.unwrap_or(10_000);
                let escaped = expression.replace('\\', "\\\\").replace('\'', "\\'");
                format!(
                    r#"new Promise((resolve, reject) => {{
    const timeout = {timeout};
    const start = Date.now();
    const check = () => {{
        try {{
            const result = (new Function('return (' + '{escaped}' + ')'))();
            if (result) {{
                resolve(true);
            }} else if (Date.now() - start > timeout) {{
                reject(new Error('Timeout waiting for function to return true'));
            }} else {{
                setTimeout(check, 100);
            }}
        }} catch (e) {{
            if (Date.now() - start > timeout) {{
                reject(new Error('Function error: ' + e.message));
            }} else {{
                setTimeout(check, 100);
            }}
        }}
    }};
    check();
}})"#
                )
            }
        }
    }
}

// ============================================================================
// Built-in Extraction Scripts
// ============================================================================

/// JavaScript to extract structured data (JSON-LD, microdata, RDFa) from a page.
pub fn extract_structured_data_script() -> &'static str {
    r#"(() => {
    const result = { jsonLd: [], microdata: [], meta: {} };

    // JSON-LD
    document.querySelectorAll('script[type="application/ld+json"]').forEach(el => {
        try {
            result.jsonLd.push(JSON.parse(el.textContent));
        } catch (e) { /* skip malformed JSON-LD */ }
    });

    // Microdata
    document.querySelectorAll('[itemscope]').forEach(el => {
        const item = { type: el.getAttribute('itemtype') || '', properties: {} };
        el.querySelectorAll('[itemprop]').forEach(prop => {
            const name = prop.getAttribute('itemprop');
            const value = prop.getAttribute('content')
                || prop.getAttribute('href')
                || prop.getAttribute('src')
                || prop.textContent.trim();
            if (item.properties[name]) {
                if (!Array.isArray(item.properties[name])) {
                    item.properties[name] = [item.properties[name]];
                }
                item.properties[name].push(value);
            } else {
                item.properties[name] = value;
            }
        });
        result.microdata.push(item);
    });

    // Open Graph and Twitter Card meta tags
    document.querySelectorAll('meta[property^="og:"], meta[name^="twitter:"]').forEach(el => {
        const key = el.getAttribute('property') || el.getAttribute('name');
        result.meta[key] = el.getAttribute('content');
    });

    // Standard meta tags
    document.querySelectorAll('meta[name="description"], meta[name="author"], meta[name="keywords"]').forEach(el => {
        result.meta[el.getAttribute('name')] = el.getAttribute('content');
    });

    return JSON.stringify(result);
})()"#
}

/// JavaScript to extract visible text content from a page.
pub fn extract_content_script() -> &'static str {
    r#"(() => {
    const result = {
        title: document.title || '',
        url: window.location.href,
        text: '',
        headings: [],
        language: document.documentElement.lang || ''
    };

    // Extract main content text
    const mainEl = document.querySelector('main, [role="main"], article, .content, #content');
    if (mainEl) {
        result.text = mainEl.innerText.trim();
    } else {
        result.text = document.body.innerText.trim();
    }

    // Extract headings hierarchy
    document.querySelectorAll('h1, h2, h3, h4, h5, h6').forEach(h => {
        result.headings.push({
            level: parseInt(h.tagName.charAt(1)),
            text: h.textContent.trim()
        });
    });

    return JSON.stringify(result);
})()"#
}

/// JavaScript to detect and describe forms on a page.
pub fn detect_forms_script() -> &'static str {
    r#"(() => {
    const forms = [];
    document.querySelectorAll('form').forEach((form, index) => {
        const fields = [];
        form.querySelectorAll('input, select, textarea, button').forEach(el => {
            const field = {
                tag: el.tagName.toLowerCase(),
                type: el.getAttribute('type') || (el.tagName === 'TEXTAREA' ? 'textarea' : el.tagName === 'SELECT' ? 'select' : 'text'),
                name: el.getAttribute('name') || '',
                id: el.getAttribute('id') || '',
                placeholder: el.getAttribute('placeholder') || '',
                required: el.hasAttribute('required'),
                value: el.value || '',
                label: ''
            };

            // Find associated label
            if (el.id) {
                const label = document.querySelector('label[for="' + el.id + '"]');
                if (label) field.label = label.textContent.trim();
            }
            if (!field.label) {
                const parent = el.closest('label');
                if (parent) field.label = parent.textContent.trim();
            }

            // For select elements, extract options
            if (el.tagName === 'SELECT') {
                field.options = Array.from(el.options).map(opt => ({
                    value: opt.value,
                    text: opt.textContent.trim(),
                    selected: opt.selected
                }));
            }

            fields.push(field);
        });

        forms.push({
            index: index,
            id: form.getAttribute('id') || '',
            name: form.getAttribute('name') || '',
            action: form.getAttribute('action') || '',
            method: (form.getAttribute('method') || 'GET').toUpperCase(),
            fields: fields
        });
    });

    return JSON.stringify(forms);
})()"#
}

/// JavaScript to extract all links from a page.
pub fn extract_links_script() -> &'static str {
    r#"(() => {
    const currentHost = window.location.hostname;
    const links = [];
    const seen = new Set();

    document.querySelectorAll('a[href]').forEach(a => {
        const href = a.href;
        if (!href || href.startsWith('javascript:') || href.startsWith('mailto:') || href.startsWith('tel:')) {
            return;
        }
        if (seen.has(href)) return;
        seen.add(href);

        let isExternal = false;
        try {
            const url = new URL(href, window.location.origin);
            isExternal = url.hostname !== currentHost;
        } catch (e) {
            // Relative URL, not external
        }

        links.push({
            href: href,
            text: a.textContent.trim().substring(0, 200),
            rel: a.getAttribute('rel') || null,
            is_external: isExternal
        });
    });

    return JSON.stringify(links);
})()"#
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_request_validation_empty() {
        let req = BatchRequest {
            operations: vec![],
            parallel: false,
            stop_on_error: true,
            timeout_ms: None,
        };
        assert!(req.validate().is_err());
        assert_eq!(
            req.validate().unwrap_err(),
            "Batch request must contain at least one operation"
        );
    }

    #[test]
    fn test_batch_request_validation_duplicate_ids() {
        let req = BatchRequest {
            operations: vec![
                BatchOperation {
                    id: "op1".to_string(),
                    command: BatchCommand::Navigate {
                        url: "https://example.com".to_string(),
                        tab_id: None,
                    },
                    wait_before: None,
                    delay_ms: None,
                },
                BatchOperation {
                    id: "op1".to_string(),
                    command: BatchCommand::Screenshot {
                        tab_id: None,
                        format: None,
                        full_page: None,
                    },
                    wait_before: None,
                    delay_ms: None,
                },
            ],
            parallel: false,
            stop_on_error: true,
            timeout_ms: None,
        };
        assert!(req.validate().is_err());
        assert!(req.validate().unwrap_err().contains("Duplicate"));
    }

    #[test]
    fn test_batch_request_validation_valid() {
        let req = BatchRequest {
            operations: vec![
                BatchOperation {
                    id: "step1".to_string(),
                    command: BatchCommand::Navigate {
                        url: "https://example.com".to_string(),
                        tab_id: None,
                    },
                    wait_before: None,
                    delay_ms: None,
                },
                BatchOperation {
                    id: "step2".to_string(),
                    command: BatchCommand::Click {
                        selector: "#login".to_string(),
                        tab_id: None,
                    },
                    wait_before: Some(WaitCondition::Selector {
                        selector: "#login".to_string(),
                        timeout_ms: Some(5000),
                    }),
                    delay_ms: Some(200),
                },
            ],
            parallel: false,
            stop_on_error: true,
            timeout_ms: Some(30_000),
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn test_batch_response_tracking() {
        let mut resp = BatchResponse::new();
        assert!(resp.success);

        resp.add_success("op1".to_string(), Some(serde_json::json!({"ok": true})), 100);
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

    #[test]
    fn test_batch_command_serialization() {
        let cmd = BatchCommand::Navigate {
            url: "https://example.com".to_string(),
            tab_id: Some("tab_1".to_string()),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("Navigate"));
        assert!(json.contains("https://example.com"));
        assert!(json.contains("tab_1"));
    }

    #[test]
    fn test_batch_command_deserialization() {
        let json = r##"{"type":"Click","selector":"#submit","tab_id":null}"##;
        let cmd: BatchCommand = serde_json::from_str(json).unwrap();
        match cmd {
            BatchCommand::Click { selector, tab_id } => {
                assert_eq!(selector, "#submit");
                assert!(tab_id.is_none());
            }
            _ => panic!("Expected Click command"),
        }
    }

    #[test]
    fn test_wait_condition_serialization() {
        let cond = WaitCondition::Selector {
            selector: "div.loaded".to_string(),
            timeout_ms: Some(5000),
        };
        let json = serde_json::to_string(&cond).unwrap();
        assert!(json.contains("Selector"));
        assert!(json.contains("div.loaded"));
    }

    #[test]
    fn test_wait_condition_timeout_defaults() {
        let selector_wait = WaitCondition::Selector {
            selector: "div".to_string(),
            timeout_ms: None,
        };
        assert_eq!(selector_wait.timeout_ms(), 10_000);

        let nav_wait = WaitCondition::Navigation { timeout_ms: None };
        assert_eq!(nav_wait.timeout_ms(), 30_000);

        let delay_wait = WaitCondition::Delay { ms: 500 };
        assert_eq!(delay_wait.timeout_ms(), 500);

        let custom_wait = WaitCondition::Function {
            expression: "true".to_string(),
            timeout_ms: Some(3000),
        };
        assert_eq!(custom_wait.timeout_ms(), 3000);
    }

    #[test]
    fn test_wait_condition_js_expression_selector() {
        let cond = WaitCondition::Selector {
            selector: "#my-element".to_string(),
            timeout_ms: Some(5000),
        };
        let js = cond.to_js_expression();
        assert!(js.contains("querySelector"));
        assert!(js.contains("#my-element"));
        assert!(js.contains("5000"));
    }

    #[test]
    fn test_wait_condition_js_expression_delay() {
        let cond = WaitCondition::Delay { ms: 1500 };
        let js = cond.to_js_expression();
        assert!(js.contains("setTimeout"));
        assert!(js.contains("1500"));
    }

    #[test]
    fn test_wait_condition_js_expression_function() {
        let cond = WaitCondition::Function {
            expression: "document.readyState === 'complete'".to_string(),
            timeout_ms: Some(8000),
        };
        let js = cond.to_js_expression();
        assert!(js.contains("8000"));
        assert!(js.contains("readyState"));
    }

    #[test]
    fn test_to_ipc_commands() {
        let req = BatchRequest {
            operations: vec![
                BatchOperation {
                    id: "nav".to_string(),
                    command: BatchCommand::Navigate {
                        url: "https://example.com".to_string(),
                        tab_id: None,
                    },
                    wait_before: None,
                    delay_ms: None,
                },
                BatchOperation {
                    id: "wait".to_string(),
                    command: BatchCommand::Wait {
                        condition: WaitCondition::Delay { ms: 500 },
                    },
                    wait_before: None,
                    delay_ms: None,
                },
                BatchOperation {
                    id: "click".to_string(),
                    command: BatchCommand::Click {
                        selector: "button".to_string(),
                        tab_id: None,
                    },
                    wait_before: None,
                    delay_ms: None,
                },
            ],
            parallel: false,
            stop_on_error: true,
            timeout_ms: None,
        };

        let cmds = req.to_ipc_commands(Some("tab_1"));
        // Wait commands are filtered out
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].0, "nav");
        assert_eq!(cmds[1].0, "click");
    }

    #[test]
    fn test_extract_options_default() {
        let opts = ExtractOptions::default();
        assert!(!opts.screenshot);
        assert!(!opts.html);
        assert!(!opts.text);
        assert!(!opts.metadata);
        assert!(!opts.structured_data);
        assert!(!opts.forms);
        assert!(!opts.links);
    }

    #[test]
    fn test_batch_request_json_roundtrip() {
        let req = BatchRequest {
            operations: vec![
                BatchOperation {
                    id: "step1".to_string(),
                    command: BatchCommand::Navigate {
                        url: "https://example.com".to_string(),
                        tab_id: Some("tab_1".to_string()),
                    },
                    wait_before: None,
                    delay_ms: Some(100),
                },
                BatchOperation {
                    id: "step2".to_string(),
                    command: BatchCommand::Type {
                        selector: "#search".to_string(),
                        text: "hello world".to_string(),
                        tab_id: None,
                        clear_first: Some(true),
                    },
                    wait_before: Some(WaitCondition::Selector {
                        selector: "#search".to_string(),
                        timeout_ms: Some(5000),
                    }),
                    delay_ms: None,
                },
            ],
            parallel: false,
            stop_on_error: true,
            timeout_ms: Some(60_000),
        };

        let json = serde_json::to_string(&req).unwrap();
        let deserialized: BatchRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.operations.len(), 2);
        assert_eq!(deserialized.operations[0].id, "step1");
        assert_eq!(deserialized.operations[1].id, "step2");
        assert!(!deserialized.parallel);
        assert!(deserialized.stop_on_error);
        assert_eq!(deserialized.timeout_ms, Some(60_000));
    }

    #[test]
    fn test_page_result_serialization() {
        let result = PageResult {
            url: "https://example.com".to_string(),
            success: true,
            title: Some("Example".to_string()),
            screenshot: None,
            html: None,
            text: Some("Hello World".to_string()),
            metadata: None,
            structured_data: None,
            forms: None,
            links: Some(vec![LinkInfo {
                href: "https://other.com".to_string(),
                text: "Other Site".to_string(),
                rel: Some("noopener".to_string()),
                is_external: true,
            }]),
            error: None,
            duration_ms: 1234,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Example"));
        assert!(json.contains("Hello World"));
        assert!(json.contains("Other Site"));
        // None fields should be omitted
        assert!(!json.contains("screenshot"));
        assert!(!json.contains("\"html\""));
    }

    #[test]
    fn test_extract_structured_data_script_is_valid_js() {
        let script = extract_structured_data_script();
        assert!(script.contains("jsonLd"));
        assert!(script.contains("microdata"));
        assert!(script.contains("application/ld+json"));
        assert!(script.contains("JSON.stringify"));
    }

    #[test]
    fn test_extract_content_script_is_valid_js() {
        let script = extract_content_script();
        assert!(script.contains("document.title"));
        assert!(script.contains("innerText"));
        assert!(script.contains("headings"));
        assert!(script.contains("JSON.stringify"));
    }

    #[test]
    fn test_detect_forms_script_is_valid_js() {
        let script = detect_forms_script();
        assert!(script.contains("querySelectorAll"));
        assert!(script.contains("form"));
        assert!(script.contains("input"));
        assert!(script.contains("JSON.stringify"));
    }

    #[test]
    fn test_extract_links_script_is_valid_js() {
        let script = extract_links_script();
        assert!(script.contains("a[href]"));
        assert!(script.contains("is_external"));
        assert!(script.contains("JSON.stringify"));
    }

    #[test]
    fn test_batch_navigate_extract_deserialization() {
        let json = r#"{
            "urls": ["https://a.com", "https://b.com"],
            "parallel_limit": 3,
            "extract": {
                "screenshot": true,
                "text": true,
                "links": true
            },
            "wait_after_navigate_ms": 2000
        }"#;

        let req: BatchNavigateExtract = serde_json::from_str(json).unwrap();
        assert_eq!(req.urls.len(), 2);
        assert_eq!(req.parallel_limit, Some(3));
        assert!(req.extract.screenshot);
        assert!(req.extract.text);
        assert!(req.extract.links);
        assert!(!req.extract.html);
        assert_eq!(req.wait_after_navigate_ms, Some(2000));
    }

    #[test]
    fn test_link_info_serialization() {
        let link = LinkInfo {
            href: "https://example.com/page".to_string(),
            text: "Click here".to_string(),
            rel: None,
            is_external: false,
        };
        let json = serde_json::to_string(&link).unwrap();
        assert!(json.contains("Click here"));
        // rel is None, should be omitted
        assert!(!json.contains("\"rel\""));
    }
}
