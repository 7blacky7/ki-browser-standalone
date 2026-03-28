//! Batch request validation and IPC command conversion.
//!
//! Implements validation logic for `BatchRequest` (duplicate IDs, empty
//! operations) and conversion of batch commands into IPC commands for
//! dispatch through the browser engine's IPC channel.

use super::scripts::{
    detect_forms_script, extract_content_script, extract_structured_data_script,
};
use super::types::{BatchCommand, BatchRequest};

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
    pub fn to_ipc_commands(
        &self,
        default_tab_id: Option<&str>,
    ) -> Vec<(String, crate::api::ipc::IpcCommand)> {
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
                            frame_id: None,
                        }
                    }
                    BatchCommand::Type {
                        selector,
                        text,
                        tab_id,
                        clear_first,
                    } => crate::api::ipc::IpcCommand::TypeText {
                        tab_id: tab(tab_id),
                        text: text.clone(),
                        selector: Some(selector.clone()),
                        clear_first: clear_first.unwrap_or(false),
                        frame_id: None,
                    },
                    BatchCommand::Screenshot {
                        tab_id,
                        format,
                        full_page,
                    } => crate::api::ipc::IpcCommand::CaptureScreenshot {
                        tab_id: tab(tab_id),
                        format: format.clone().unwrap_or_else(|| "png".to_string()),
                        quality: None,
                        full_page: full_page.unwrap_or(false),
                        selector: None,
                        clip_x: None,
                        clip_y: None,
                        clip_width: None,
                        clip_height: None,
                        clip_scale: None,
                    },
                    BatchCommand::Evaluate { script, tab_id } => {
                        crate::api::ipc::IpcCommand::EvaluateScript {
                            tab_id: tab(tab_id),
                            script: script.clone(),
                            await_promise: true,
                            frame_id: None,
                        }
                    }
                    BatchCommand::Scroll {
                        delta_x,
                        delta_y,
                        tab_id,
                    } => crate::api::ipc::IpcCommand::Scroll {
                        tab_id: tab(tab_id),
                        x: None,
                        y: None,
                        delta_x: delta_x.map(|v| v as i32),
                        delta_y: delta_y.map(|v| v as i32),
                        selector: None,
                        behavior: None,
                        frame_id: None,
                    },
                    BatchCommand::Wait { .. } => {
                        // Wait operations are handled by the executor, not IPC
                        return None;
                    }
                    BatchCommand::ExtractStructuredData { tab_id } => {
                        crate::api::ipc::IpcCommand::EvaluateScript {
                            tab_id: tab(tab_id),
                            script: extract_structured_data_script().to_string(),
                            await_promise: true,
                            frame_id: None,
                        }
                    }
                    BatchCommand::ExtractContent { tab_id } => {
                        crate::api::ipc::IpcCommand::EvaluateScript {
                            tab_id: tab(tab_id),
                            script: extract_content_script().to_string(),
                            await_promise: true,
                            frame_id: None,
                        }
                    }
                    BatchCommand::DetectForms { tab_id } => {
                        crate::api::ipc::IpcCommand::EvaluateScript {
                            tab_id: tab(tab_id),
                            script: detect_forms_script().to_string(),
                            await_promise: true,
                            frame_id: None,
                        }
                    }
                    BatchCommand::NewTab { url } => crate::api::ipc::IpcCommand::CreateTab {
                        url: url.clone().unwrap_or_else(|| "about:blank".to_string()),
                        active: true,
                    },
                    BatchCommand::CloseTab { tab_id } => crate::api::ipc::IpcCommand::CloseTab {
                        tab_id: tab_id.clone(),
                    },
                };

                Some((op.id.clone(), cmd))
            })
            .collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::super::types::*;

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
