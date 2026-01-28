//! DOM access abstraction for browser automation.
//!
//! This module provides structures and traits for accessing and manipulating
//! DOM elements in the browser. It includes abstractions for element selection,
//! attribute access, and JavaScript evaluation.
//!
//! # Example
//!
//! ```rust,ignore
//! use ki_browser::browser::dom::{DomAccessor, MockDomAccessor, DomElement};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let accessor = MockDomAccessor::new();
//!
//!     let element = accessor.find_element("#search-input").await?;
//!     if let Some(el) = element {
//!         println!("Found element: {}", el.tag_name);
//!     }
//!
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents the bounding box of a DOM element.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    /// X coordinate (left edge) in pixels.
    pub x: f64,

    /// Y coordinate (top edge) in pixels.
    pub y: f64,

    /// Width in pixels.
    pub width: f64,

    /// Height in pixels.
    pub height: f64,
}

impl BoundingBox {
    /// Creates a new BoundingBox.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Returns the center point of the bounding box.
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Returns the right edge coordinate.
    pub fn right(&self) -> f64 {
        self.x + self.width
    }

    /// Returns the bottom edge coordinate.
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }

    /// Checks if a point is within the bounding box.
    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }

    /// Checks if this bounding box intersects with another.
    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    /// Returns the area of the bounding box.
    pub fn area(&self) -> f64 {
        self.width * self.height
    }

    /// Checks if the bounding box is visible (has positive dimensions).
    pub fn is_visible(&self) -> bool {
        self.width > 0.0 && self.height > 0.0
    }
}

impl Default for BoundingBox {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }
}

/// Represents a DOM element with its properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomElement {
    /// CSS selector that uniquely identifies this element.
    pub selector: String,

    /// HTML tag name (e.g., "div", "input", "button").
    pub tag_name: String,

    /// Element attributes as key-value pairs.
    pub attributes: HashMap<String, String>,

    /// Text content of the element (may be empty).
    pub text_content: String,

    /// Inner HTML content.
    pub inner_html: String,

    /// Bounding box coordinates and dimensions.
    pub bounding_box: Option<BoundingBox>,

    /// Whether the element is visible on the page.
    pub is_visible: bool,

    /// Whether the element is currently enabled (for form elements).
    pub is_enabled: bool,

    /// Whether the element is focusable.
    pub is_focusable: bool,

    /// Node ID from the browser's DOM tree (implementation-specific).
    pub node_id: Option<i64>,

    /// Backend node ID for CDP operations.
    pub backend_node_id: Option<i64>,
}

impl DomElement {
    /// Creates a new DomElement with minimal required fields.
    pub fn new(selector: String, tag_name: String) -> Self {
        Self {
            selector,
            tag_name,
            attributes: HashMap::new(),
            text_content: String::new(),
            inner_html: String::new(),
            bounding_box: None,
            is_visible: true,
            is_enabled: true,
            is_focusable: false,
            node_id: None,
            backend_node_id: None,
        }
    }

    /// Gets an attribute value by name.
    pub fn get_attribute(&self, name: &str) -> Option<&String> {
        self.attributes.get(name)
    }

    /// Checks if the element has a specific attribute.
    pub fn has_attribute(&self, name: &str) -> bool {
        self.attributes.contains_key(name)
    }

    /// Gets the element's ID attribute.
    pub fn id(&self) -> Option<&String> {
        self.attributes.get("id")
    }

    /// Gets the element's class attribute.
    pub fn class(&self) -> Option<&String> {
        self.attributes.get("class")
    }

    /// Gets the element's classes as a vector.
    pub fn classes(&self) -> Vec<&str> {
        self.attributes
            .get("class")
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default()
    }

    /// Checks if the element has a specific class.
    pub fn has_class(&self, class_name: &str) -> bool {
        self.classes().contains(&class_name)
    }

    /// Gets the element's value attribute (for form elements).
    pub fn value(&self) -> Option<&String> {
        self.attributes.get("value")
    }

    /// Gets the element's href attribute (for links).
    pub fn href(&self) -> Option<&String> {
        self.attributes.get("href")
    }

    /// Gets the element's src attribute (for images, scripts, etc.).
    pub fn src(&self) -> Option<&String> {
        self.attributes.get("src")
    }

    /// Checks if this is an input element.
    pub fn is_input(&self) -> bool {
        self.tag_name.eq_ignore_ascii_case("input")
    }

    /// Checks if this is a button element.
    pub fn is_button(&self) -> bool {
        self.tag_name.eq_ignore_ascii_case("button")
            || (self.is_input()
                && self
                    .get_attribute("type")
                    .map(|t| t == "button" || t == "submit")
                    .unwrap_or(false))
    }

    /// Checks if this is a link element.
    pub fn is_link(&self) -> bool {
        self.tag_name.eq_ignore_ascii_case("a")
    }

    /// Checks if this is a form element.
    pub fn is_form_element(&self) -> bool {
        matches!(
            self.tag_name.to_lowercase().as_str(),
            "input" | "textarea" | "select" | "button"
        )
    }

    /// Returns the center point if bounding box is available.
    pub fn center(&self) -> Option<(f64, f64)> {
        self.bounding_box.as_ref().map(|bb| bb.center())
    }
}

/// Represents the result of JavaScript evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsValue {
    /// Null value.
    Null,
    /// Boolean value.
    Bool(bool),
    /// Numeric value.
    Number(f64),
    /// String value.
    String(String),
    /// Array of values.
    Array(Vec<JsValue>),
    /// Object with string keys.
    Object(HashMap<String, JsValue>),
    /// Undefined value.
    Undefined,
}

impl JsValue {
    /// Attempts to get this value as a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Attempts to get this value as a number.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            JsValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Attempts to get this value as a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            JsValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Attempts to get this value as an array.
    pub fn as_array(&self) -> Option<&Vec<JsValue>> {
        match self {
            JsValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Attempts to get this value as an object.
    pub fn as_object(&self) -> Option<&HashMap<String, JsValue>> {
        match self {
            JsValue::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// Returns true if this value is null or undefined.
    pub fn is_nullish(&self) -> bool {
        matches!(self, JsValue::Null | JsValue::Undefined)
    }
}

/// Trait for accessing DOM elements in a browser tab.
///
/// This trait provides an abstraction for DOM access operations,
/// allowing different implementations for different browser engines.
#[async_trait]
pub trait DomAccessor: Send + Sync {
    /// Finds a single element matching the given CSS selector.
    ///
    /// # Arguments
    ///
    /// * `selector` - CSS selector string
    ///
    /// # Returns
    ///
    /// The first matching element, or None if not found.
    async fn find_element(&self, selector: &str) -> Result<Option<DomElement>>;

    /// Finds all elements matching the given CSS selector.
    ///
    /// # Arguments
    ///
    /// * `selector` - CSS selector string
    ///
    /// # Returns
    ///
    /// A vector of matching elements (may be empty).
    async fn find_elements(&self, selector: &str) -> Result<Vec<DomElement>>;

    /// Gets an attribute value from an element.
    ///
    /// # Arguments
    ///
    /// * `selector` - CSS selector for the element
    /// * `attribute` - Name of the attribute to retrieve
    ///
    /// # Returns
    ///
    /// The attribute value, or None if the element or attribute doesn't exist.
    async fn get_attribute(&self, selector: &str, attribute: &str) -> Result<Option<String>>;

    /// Evaluates JavaScript code in the browser context.
    ///
    /// # Arguments
    ///
    /// * `script` - JavaScript code to evaluate
    ///
    /// # Returns
    ///
    /// The result of the JavaScript evaluation.
    async fn evaluate_js(&self, script: &str) -> Result<JsValue>;

    /// Waits for an element to appear in the DOM.
    ///
    /// # Arguments
    ///
    /// * `selector` - CSS selector for the element
    /// * `timeout_ms` - Maximum time to wait in milliseconds
    ///
    /// # Returns
    ///
    /// The element if found within the timeout, or an error.
    async fn wait_for_element(&self, selector: &str, timeout_ms: u64) -> Result<DomElement>;

    /// Gets the text content of an element.
    ///
    /// # Arguments
    ///
    /// * `selector` - CSS selector for the element
    ///
    /// # Returns
    ///
    /// The text content, or None if the element doesn't exist.
    async fn get_text_content(&self, selector: &str) -> Result<Option<String>>;

    /// Gets the inner HTML of an element.
    ///
    /// # Arguments
    ///
    /// * `selector` - CSS selector for the element
    ///
    /// # Returns
    ///
    /// The inner HTML, or None if the element doesn't exist.
    async fn get_inner_html(&self, selector: &str) -> Result<Option<String>>;

    /// Checks if an element exists in the DOM.
    ///
    /// # Arguments
    ///
    /// * `selector` - CSS selector for the element
    async fn element_exists(&self, selector: &str) -> Result<bool>;

    /// Checks if an element is visible on the page.
    ///
    /// # Arguments
    ///
    /// * `selector` - CSS selector for the element
    async fn is_element_visible(&self, selector: &str) -> Result<bool>;
}

/// Mock DOM accessor for testing purposes.
///
/// This implementation provides simulated DOM access for unit tests,
/// with configurable mock elements and responses.
pub struct MockDomAccessor {
    /// Mock elements that can be "found" by selectors.
    elements: std::sync::RwLock<HashMap<String, Vec<DomElement>>>,

    /// Mock JavaScript evaluation results.
    js_results: std::sync::RwLock<HashMap<String, JsValue>>,
}

impl Default for MockDomAccessor {
    fn default() -> Self {
        Self::new()
    }
}

impl MockDomAccessor {
    /// Creates a new MockDomAccessor with no pre-configured elements.
    pub fn new() -> Self {
        Self {
            elements: std::sync::RwLock::new(HashMap::new()),
            js_results: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Adds mock elements for a selector.
    pub fn add_elements(&self, selector: &str, elements: Vec<DomElement>) {
        let mut map = self.elements.write().unwrap();
        map.insert(selector.to_string(), elements);
    }

    /// Adds a single mock element for a selector.
    pub fn add_element(&self, selector: &str, element: DomElement) {
        self.add_elements(selector, vec![element]);
    }

    /// Sets a mock JavaScript result for a script.
    pub fn set_js_result(&self, script: &str, result: JsValue) {
        let mut map = self.js_results.write().unwrap();
        map.insert(script.to_string(), result);
    }

    /// Creates a simple mock element with basic properties.
    pub fn create_mock_element(selector: &str, tag_name: &str, text_content: &str) -> DomElement {
        let mut element = DomElement::new(selector.to_string(), tag_name.to_string());
        element.text_content = text_content.to_string();
        element.bounding_box = Some(BoundingBox::new(100.0, 100.0, 200.0, 50.0));
        element
    }

    /// Clears all mock elements and JS results.
    pub fn clear(&self) {
        self.elements.write().unwrap().clear();
        self.js_results.write().unwrap().clear();
    }
}

#[async_trait]
impl DomAccessor for MockDomAccessor {
    async fn find_element(&self, selector: &str) -> Result<Option<DomElement>> {
        let map = self.elements.read().unwrap();
        Ok(map.get(selector).and_then(|v| v.first().cloned()))
    }

    async fn find_elements(&self, selector: &str) -> Result<Vec<DomElement>> {
        let map = self.elements.read().unwrap();
        Ok(map.get(selector).cloned().unwrap_or_default())
    }

    async fn get_attribute(&self, selector: &str, attribute: &str) -> Result<Option<String>> {
        let element = self.find_element(selector).await?;
        Ok(element.and_then(|e| e.attributes.get(attribute).cloned()))
    }

    async fn evaluate_js(&self, script: &str) -> Result<JsValue> {
        let map = self.js_results.read().unwrap();
        Ok(map.get(script).cloned().unwrap_or(JsValue::Undefined))
    }

    async fn wait_for_element(&self, selector: &str, _timeout_ms: u64) -> Result<DomElement> {
        self.find_element(selector)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Element not found: {}", selector))
    }

    async fn get_text_content(&self, selector: &str) -> Result<Option<String>> {
        let element = self.find_element(selector).await?;
        Ok(element.map(|e| e.text_content))
    }

    async fn get_inner_html(&self, selector: &str) -> Result<Option<String>> {
        let element = self.find_element(selector).await?;
        Ok(element.map(|e| e.inner_html))
    }

    async fn element_exists(&self, selector: &str) -> Result<bool> {
        Ok(self.find_element(selector).await?.is_some())
    }

    async fn is_element_visible(&self, selector: &str) -> Result<bool> {
        let element = self.find_element(selector).await?;
        Ok(element.map(|e| e.is_visible).unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounding_box() {
        let bb = BoundingBox::new(10.0, 20.0, 100.0, 50.0);

        assert_eq!(bb.center(), (60.0, 45.0));
        assert_eq!(bb.right(), 110.0);
        assert_eq!(bb.bottom(), 70.0);
        assert_eq!(bb.area(), 5000.0);
        assert!(bb.is_visible());

        assert!(bb.contains_point(50.0, 40.0));
        assert!(!bb.contains_point(0.0, 0.0));

        let other = BoundingBox::new(50.0, 30.0, 100.0, 100.0);
        assert!(bb.intersects(&other));

        let non_overlapping = BoundingBox::new(200.0, 200.0, 50.0, 50.0);
        assert!(!bb.intersects(&non_overlapping));
    }

    #[test]
    fn test_dom_element() {
        let mut element = DomElement::new("#test".to_string(), "div".to_string());
        element.attributes.insert("id".to_string(), "test".to_string());
        element
            .attributes
            .insert("class".to_string(), "foo bar baz".to_string());
        element.text_content = "Hello World".to_string();
        element.bounding_box = Some(BoundingBox::new(0.0, 0.0, 100.0, 50.0));

        assert_eq!(element.id(), Some(&"test".to_string()));
        assert!(element.has_class("foo"));
        assert!(element.has_class("bar"));
        assert!(!element.has_class("qux"));
        assert_eq!(element.classes(), vec!["foo", "bar", "baz"]);
        assert_eq!(element.center(), Some((50.0, 25.0)));
    }

    #[test]
    fn test_dom_element_types() {
        let input = DomElement::new("#input".to_string(), "input".to_string());
        assert!(input.is_input());
        assert!(input.is_form_element());
        assert!(!input.is_button());

        let mut button_input = DomElement::new("#btn".to_string(), "input".to_string());
        button_input
            .attributes
            .insert("type".to_string(), "button".to_string());
        assert!(button_input.is_button());

        let button = DomElement::new("#btn2".to_string(), "button".to_string());
        assert!(button.is_button());

        let link = DomElement::new("#link".to_string(), "a".to_string());
        assert!(link.is_link());
    }

    #[test]
    fn test_js_value() {
        let null = JsValue::Null;
        assert!(null.is_nullish());

        let bool_val = JsValue::Bool(true);
        assert_eq!(bool_val.as_bool(), Some(true));

        let num_val = JsValue::Number(42.5);
        assert_eq!(num_val.as_number(), Some(42.5));

        let str_val = JsValue::String("hello".to_string());
        assert_eq!(str_val.as_str(), Some("hello"));

        let arr_val = JsValue::Array(vec![JsValue::Number(1.0), JsValue::Number(2.0)]);
        assert_eq!(arr_val.as_array().map(|a| a.len()), Some(2));

        let mut obj = HashMap::new();
        obj.insert("key".to_string(), JsValue::String("value".to_string()));
        let obj_val = JsValue::Object(obj);
        assert!(obj_val.as_object().is_some());
    }

    #[tokio::test]
    async fn test_mock_dom_accessor() {
        let accessor = MockDomAccessor::new();

        let mut element = DomElement::new("#test".to_string(), "div".to_string());
        element.text_content = "Test Content".to_string();
        element
            .attributes
            .insert("data-test".to_string(), "value".to_string());

        accessor.add_element("#test", element);

        let found = accessor.find_element("#test").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.as_ref().unwrap().text_content, "Test Content");

        let attr = accessor.get_attribute("#test", "data-test").await.unwrap();
        assert_eq!(attr, Some("value".to_string()));

        assert!(accessor.element_exists("#test").await.unwrap());
        assert!(!accessor.element_exists("#nonexistent").await.unwrap());

        let text = accessor.get_text_content("#test").await.unwrap();
        assert_eq!(text, Some("Test Content".to_string()));
    }

    #[tokio::test]
    async fn test_mock_dom_accessor_js() {
        let accessor = MockDomAccessor::new();

        accessor.set_js_result("document.title", JsValue::String("Test Page".to_string()));

        let result = accessor.evaluate_js("document.title").await.unwrap();
        assert_eq!(result.as_str(), Some("Test Page"));

        let undefined = accessor.evaluate_js("unknown").await.unwrap();
        assert!(undefined.is_nullish());
    }
}
