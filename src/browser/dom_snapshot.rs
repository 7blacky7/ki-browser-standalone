//! DOM snapshot capture with bounding-box information for KI agent vision.
//!
//! Provides a lightweight, serializable representation of the visible DOM tree
//! including bounding rectangles, ARIA roles, and interactivity flags. Used by
//! the vision overlay system to map visual regions to actionable DOM elements.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::browser::dom::BoundingBox;
use crate::error::{BrowserError, BrowserResult};

/// Default maximum number of DOM nodes captured in a single snapshot.
pub const DEFAULT_MAX_NODES: u32 = 1000;

/// JavaScript source for DOM tree traversal with bounding-box extraction.
///
/// Uses TreeWalker over `document.body`, calls `getBoundingClientRect()` on each
/// visible node, filters invisible elements, detects interactive elements, and
/// reads ARIA roles. Returns a JSON string limited to `maxNodes` entries.
pub const DOM_SNAPSHOT_JS: &str = r#"
(function(maxNodes, includeText) {
    var nodes = [];
    var nodeId = 0;
    var parentStack = [];

    var INTERACTIVE_TAGS = {
        'A': true, 'BUTTON': true, 'INPUT': true, 'SELECT': true,
        'TEXTAREA': true, 'DETAILS': true, 'SUMMARY': true, 'LABEL': true
    };

    function isInteractive(el) {
        if (INTERACTIVE_TAGS[el.tagName]) return true;
        if (el.hasAttribute('onclick') || el.hasAttribute('onmousedown') ||
            el.hasAttribute('onmouseup') || el.hasAttribute('ontouchstart')) return true;
        var role = el.getAttribute('role');
        if (role === 'button' || role === 'link' || role === 'tab' ||
            role === 'menuitem' || role === 'checkbox' || role === 'radio' ||
            role === 'switch' || role === 'textbox' || role === 'combobox' ||
            role === 'option' || role === 'slider') return true;
        if (el.hasAttribute('tabindex') && el.getAttribute('tabindex') !== '-1') return true;
        if (el.contentEditable === 'true') return true;
        return false;
    }

    function getVisibleText(el) {
        if (!includeText) return null;
        var text = '';
        for (var i = 0; i < el.childNodes.length; i++) {
            if (el.childNodes[i].nodeType === 3) {
                text += el.childNodes[i].textContent;
            }
        }
        text = text.trim();
        return text.length > 0 ? text.substring(0, 500) : null;
    }

    function getAttributes(el) {
        var attrs = {};
        for (var i = 0; i < el.attributes.length; i++) {
            var attr = el.attributes[i];
            if (attr.name !== 'style') {
                attrs[attr.name] = attr.value;
            }
        }
        return attrs;
    }

    function traverse(el, parentId) {
        if (nodeId >= maxNodes) return;

        var style = window.getComputedStyle(el);
        if (style.display === 'none' || style.visibility === 'hidden') return;

        var rect = el.getBoundingClientRect();
        if (rect.width === 0 && rect.height === 0) return;

        var id = nodeId++;
        var isVisible = rect.width > 0 && rect.height > 0 &&
            style.opacity !== '0' && rect.bottom > 0 && rect.right > 0 &&
            rect.top < window.innerHeight && rect.left < window.innerWidth;

        var node = {
            id: id,
            tag: el.tagName.toLowerCase(),
            attributes: getAttributes(el),
            text: getVisibleText(el),
            bbox: {
                x: Math.round(rect.x * 100) / 100,
                y: Math.round(rect.y * 100) / 100,
                width: Math.round(rect.width * 100) / 100,
                height: Math.round(rect.height * 100) / 100
            },
            children: [],
            role: el.getAttribute('role') || null,
            is_visible: isVisible,
            is_interactive: isInteractive(el),
            parent_id: parentId
        };

        nodes.push(node);

        var children = el.children;
        for (var i = 0; i < children.length; i++) {
            if (nodeId >= maxNodes) break;
            var childStartId = nodeId;
            traverse(children[i], id);
            if (nodeId > childStartId) {
                node.children.push(childStartId);
            }
        }
    }

    if (document.body) {
        traverse(document.body, null);
    }

    return JSON.stringify({
        nodes: nodes,
        viewport: {
            width: window.innerWidth,
            height: window.innerHeight,
            scroll_x: window.scrollX,
            scroll_y: window.scrollY
        },
        device_pixel_ratio: window.devicePixelRatio || 1.0,
        url: window.location.href,
        timestamp: new Date().toISOString()
    });
})
"#;

// -------------------------------------------------------------------------
// Data structures
// -------------------------------------------------------------------------

/// Lightweight representation of a single DOM node inside a snapshot.
///
/// Unlike [`crate::browser::dom::DomElement`] which is designed for interactive
/// DOM queries, `DomNode` is a read-only, serializable record optimized for
/// vision-based KI agent consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomNode {
    /// Sequential identifier unique within this snapshot (0-based).
    pub id: u32,

    /// Lowercase HTML tag name (e.g. "div", "button", "input").
    pub tag: String,

    /// Element attributes (excluding `style`).
    pub attributes: HashMap<String, String>,

    /// Direct text content of the node (first 500 chars), if requested.
    pub text: Option<String>,

    /// Bounding rectangle in viewport coordinates from `getBoundingClientRect()`.
    pub bbox: BoundingBox,

    /// IDs of direct child nodes within this snapshot.
    pub children: Vec<u32>,

    /// ARIA `role` attribute value, if present.
    pub role: Option<String>,

    /// Whether the element is within the visible viewport area.
    pub is_visible: bool,

    /// Whether the element is interactive (clickable, editable, focusable).
    pub is_interactive: bool,

    /// Parent node ID within this snapshot, `None` for root.
    pub parent_id: Option<u32>,
}

/// Viewport dimensions and scroll position at the time of snapshot capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewportInfo {
    /// Viewport width in CSS pixels.
    pub width: u32,

    /// Viewport height in CSS pixels.
    pub height: u32,

    /// Horizontal scroll offset in CSS pixels.
    pub scroll_x: f64,

    /// Vertical scroll offset in CSS pixels.
    pub scroll_y: f64,
}

/// Complete DOM snapshot including all captured nodes, viewport state, and metadata.
///
/// This struct is the primary output of the `dom_snapshot` operation. It is designed
/// to be serialized as JSON for consumption by KI agent vision systems and the
/// overlay renderer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomSnapshot {
    /// Captured DOM nodes ordered by their `id` field.
    pub nodes: Vec<DomNode>,

    /// Viewport dimensions and scroll position at capture time.
    pub viewport: ViewportInfo,

    /// Device pixel ratio (e.g. 2.0 for Retina displays).
    pub device_pixel_ratio: f64,

    /// Page URL at capture time.
    pub url: String,

    /// UTC timestamp of the capture.
    pub timestamp: DateTime<Utc>,
}

/// Configuration options for DOM snapshot capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConfig {
    /// Maximum number of nodes to capture (default: 1000).
    pub max_nodes: u32,

    /// Whether to include text content of nodes.
    pub include_text: bool,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            max_nodes: DEFAULT_MAX_NODES,
            include_text: true,
        }
    }
}

// -------------------------------------------------------------------------
// Parsing
// -------------------------------------------------------------------------

/// Parses the JSON string returned by the DOM snapshot JavaScript into a [`DomSnapshot`].
///
/// The JavaScript returns a stringified JSON object; this function deserializes it
/// into the strongly-typed Rust structures. Returns a [`BrowserError::ScriptEvaluationFailed`]
/// if the JSON is malformed or missing required fields.
pub fn parse_snapshot_json(json_str: &str) -> BrowserResult<DomSnapshot> {
    #[derive(Deserialize)]
    struct RawSnapshot {
        nodes: Vec<RawNode>,
        viewport: RawViewport,
        device_pixel_ratio: f64,
        url: String,
        timestamp: String,
    }

    #[derive(Deserialize)]
    struct RawNode {
        id: u32,
        tag: String,
        #[serde(default)]
        attributes: HashMap<String, String>,
        text: Option<String>,
        bbox: RawBbox,
        #[serde(default)]
        children: Vec<u32>,
        role: Option<String>,
        is_visible: bool,
        is_interactive: bool,
        parent_id: Option<u32>,
    }

    #[derive(Deserialize)]
    struct RawBbox {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }

    #[derive(Deserialize)]
    struct RawViewport {
        width: u32,
        height: u32,
        scroll_x: f64,
        scroll_y: f64,
    }

    let raw: RawSnapshot = serde_json::from_str(json_str).map_err(|e| {
        BrowserError::ScriptEvaluationFailed {
            reason: format!("Failed to parse DOM snapshot JSON: {}", e),
        }
    })?;

    let timestamp = DateTime::parse_from_rfc3339(&raw.timestamp)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    let nodes = raw
        .nodes
        .into_iter()
        .map(|n| DomNode {
            id: n.id,
            tag: n.tag,
            attributes: n.attributes,
            text: n.text,
            bbox: BoundingBox::new(n.bbox.x, n.bbox.y, n.bbox.width, n.bbox.height),
            children: n.children,
            role: n.role,
            is_visible: n.is_visible,
            is_interactive: n.is_interactive,
            parent_id: n.parent_id,
        })
        .collect();

    Ok(DomSnapshot {
        nodes,
        viewport: ViewportInfo {
            width: raw.viewport.width,
            height: raw.viewport.height,
            scroll_x: raw.viewport.scroll_x,
            scroll_y: raw.viewport.scroll_y,
        },
        device_pixel_ratio: raw.device_pixel_ratio,
        url: raw.url,
        timestamp,
    })
}

/// Builds the full JavaScript expression for snapshot capture with given config.
pub fn build_snapshot_script(config: &SnapshotConfig) -> String {
    format!(
        "{}({}, {})",
        DOM_SNAPSHOT_JS, config.max_nodes, config.include_text
    )
}

// -------------------------------------------------------------------------
// Utility methods
// -------------------------------------------------------------------------

impl DomSnapshot {
    /// Returns only the interactive nodes from the snapshot.
    pub fn interactive_nodes(&self) -> Vec<&DomNode> {
        self.nodes.iter().filter(|n| n.is_interactive).collect()
    }

    /// Returns only the visible nodes from the snapshot.
    pub fn visible_nodes(&self) -> Vec<&DomNode> {
        self.nodes.iter().filter(|n| n.is_visible).collect()
    }

    /// Finds the node at the given viewport coordinates using bounding-box hit testing.
    ///
    /// Returns the deepest (most specific) node whose bounding box contains the point.
    pub fn node_at_point(&self, x: f64, y: f64) -> Option<&DomNode> {
        self.nodes
            .iter()
            .filter(|n| n.is_visible && n.bbox.contains_point(x, y))
            .last()
    }

    /// Finds a node by its snapshot-local ID.
    pub fn node_by_id(&self, id: u32) -> Option<&DomNode> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

// -------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot_json() -> String {
        serde_json::json!({
            "nodes": [
                {
                    "id": 0,
                    "tag": "body",
                    "attributes": {},
                    "text": null,
                    "bbox": { "x": 0.0, "y": 0.0, "width": 1920.0, "height": 1080.0 },
                    "children": [1, 2],
                    "role": null,
                    "is_visible": true,
                    "is_interactive": false,
                    "parent_id": null
                },
                {
                    "id": 1,
                    "tag": "button",
                    "attributes": { "id": "submit-btn", "class": "primary" },
                    "text": "Submit",
                    "bbox": { "x": 100.0, "y": 200.0, "width": 120.0, "height": 40.0 },
                    "children": [],
                    "role": "button",
                    "is_visible": true,
                    "is_interactive": true,
                    "parent_id": 0
                },
                {
                    "id": 2,
                    "tag": "div",
                    "attributes": { "class": "hidden-panel" },
                    "text": null,
                    "bbox": { "x": 0.0, "y": 0.0, "width": 100.0, "height": 50.0 },
                    "children": [],
                    "role": null,
                    "is_visible": false,
                    "is_interactive": false,
                    "parent_id": 0
                }
            ],
            "viewport": { "width": 1920, "height": 1080, "scroll_x": 0.0, "scroll_y": 0.0 },
            "device_pixel_ratio": 1.0,
            "url": "https://example.com",
            "timestamp": "2026-03-01T12:00:00Z"
        })
        .to_string()
    }

    #[test]
    fn test_dom_node_serialization() {
        let node = DomNode {
            id: 0,
            tag: "div".to_string(),
            attributes: HashMap::from([("class".to_string(), "container".to_string())]),
            text: Some("Hello".to_string()),
            bbox: BoundingBox::new(10.0, 20.0, 300.0, 150.0),
            children: vec![1, 2],
            role: None,
            is_visible: true,
            is_interactive: false,
            parent_id: None,
        };

        let json = serde_json::to_string(&node).unwrap();
        let deserialized: DomNode = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, 0);
        assert_eq!(deserialized.tag, "div");
        assert_eq!(deserialized.text, Some("Hello".to_string()));
        assert_eq!(deserialized.bbox.x, 10.0);
        assert_eq!(deserialized.bbox.width, 300.0);
        assert_eq!(deserialized.children, vec![1, 2]);
        assert!(deserialized.is_visible);
        assert!(!deserialized.is_interactive);
    }

    #[test]
    fn test_bbox_contains_point_in_snapshot() {
        let snapshot = parse_snapshot_json(&sample_snapshot_json()).unwrap();
        let button = &snapshot.nodes[1];

        assert_eq!(button.tag, "button");
        assert!(button.bbox.contains_point(150.0, 220.0));
        assert!(!button.bbox.contains_point(50.0, 50.0));
        assert!(button.bbox.contains_point(100.0, 200.0)); // top-left corner
        assert!(button.bbox.contains_point(220.0, 240.0)); // bottom-right corner
    }

    #[test]
    fn test_interactive_element_detection() {
        let snapshot = parse_snapshot_json(&sample_snapshot_json()).unwrap();
        let interactive = snapshot.interactive_nodes();

        assert_eq!(interactive.len(), 1);
        assert_eq!(interactive[0].tag, "button");
        assert_eq!(interactive[0].role, Some("button".to_string()));
    }

    #[test]
    fn test_snapshot_filtering() {
        let snapshot = parse_snapshot_json(&sample_snapshot_json()).unwrap();

        let visible = snapshot.visible_nodes();
        assert_eq!(visible.len(), 2); // body + button (div is hidden)

        let hidden: Vec<_> = snapshot.nodes.iter().filter(|n| !n.is_visible).collect();
        assert_eq!(hidden.len(), 1);
        assert_eq!(hidden[0].tag, "div");
    }

    #[test]
    fn test_node_at_point() {
        let snapshot = parse_snapshot_json(&sample_snapshot_json()).unwrap();

        // Point inside the button
        let node = snapshot.node_at_point(150.0, 220.0);
        assert!(node.is_some());
        assert_eq!(node.unwrap().tag, "button");

        // Point outside all visible elements
        let node = snapshot.node_at_point(1950.0, 1100.0);
        assert!(node.is_none());
    }

    #[test]
    fn test_node_by_id() {
        let snapshot = parse_snapshot_json(&sample_snapshot_json()).unwrap();

        let node = snapshot.node_by_id(1);
        assert!(node.is_some());
        assert_eq!(node.unwrap().tag, "button");

        let node = snapshot.node_by_id(99);
        assert!(node.is_none());
    }

    #[test]
    fn test_parse_snapshot_invalid_json() {
        let result = parse_snapshot_json("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_snapshot_config_default() {
        let config = SnapshotConfig::default();
        assert_eq!(config.max_nodes, 1000);
        assert!(config.include_text);
    }

    #[test]
    fn test_build_snapshot_script() {
        let config = SnapshotConfig {
            max_nodes: 500,
            include_text: false,
        };
        let script = build_snapshot_script(&config);
        assert!(script.contains("500"));
        assert!(script.contains("false"));
    }

    #[test]
    fn test_snapshot_viewport_info() {
        let snapshot = parse_snapshot_json(&sample_snapshot_json()).unwrap();
        assert_eq!(snapshot.viewport.width, 1920);
        assert_eq!(snapshot.viewport.height, 1080);
        assert_eq!(snapshot.viewport.scroll_x, 0.0);
        assert_eq!(snapshot.viewport.scroll_y, 0.0);
        assert_eq!(snapshot.device_pixel_ratio, 1.0);
        assert_eq!(snapshot.url, "https://example.com");
    }
}
