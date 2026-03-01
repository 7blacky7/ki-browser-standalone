//! Batch operations for atomic multi-request execution.
//!
//! Enables AI agents to execute multiple browser commands as a single
//! atomic unit, with support for sequential and parallel execution,
//! error handling policies, and convenience methods for common
//! multi-step workflows.

mod request;
mod response;
pub mod scripts;
pub mod types;
mod wait;

// Re-export all public types so `use crate::api::batch::*` keeps working
pub use scripts::{
    detect_forms_script, extract_content_script, extract_links_script,
    extract_structured_data_script,
};
pub use types::{
    BatchCommand, BatchNavigateExtract, BatchNavigateResult, BatchOperation,
    BatchOperationResult, BatchRequest, BatchResponse, ExtractOptions, LinkInfo, PageResult,
    WaitCondition,
};
