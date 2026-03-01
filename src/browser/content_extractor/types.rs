//! Type definitions for content extraction and page structure analysis.
//!
//! Contains serializable data structures returned by the browser-side JavaScript
//! extraction scripts: [`ExtractedContent`] for Readability-like main content,
//! [`PageStructure`] for structural breakdown, and supporting types for sections,
//! navigation elements, and page type classification.

use serde::{Deserialize, Serialize};

/// Extracted main content of a page.
///
/// Contains the primary readable content identified by the extraction
/// algorithm, along with metadata such as author, publication date,
/// and reading time estimates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedContent {
    /// Main article/content as plain text.
    pub content: String,

    /// Main content as clean HTML (stripped of scripts, styles, ads).
    pub content_html: String,

    /// Page title (from `<title>` or first `<h1>`).
    pub title: String,

    /// Detected author (from meta tags, byline patterns, or Schema.org).
    pub author: Option<String>,

    /// Publication date if found (ISO 8601 string).
    pub published_date: Option<String>,

    /// Estimated reading time in minutes (based on 200 words/minute).
    pub reading_time_minutes: u32,

    /// Total word count of the extracted content.
    pub word_count: u32,

    /// Detected language (from `<html lang="...">` attribute).
    pub language: Option<String>,

    /// Content extraction confidence score (0.0 to 1.0).
    pub score: f64,
}

/// Page structure analysis result.
///
/// Provides a structural breakdown of the page, identifying distinct
/// sections, navigation elements, and the overall page type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageStructure {
    /// Sections found on the page, classified by role.
    pub sections: Vec<PageSection>,

    /// Navigation elements (links from `<nav>` or navigation regions).
    pub navigation: Vec<NavElement>,

    /// Detected page type based on structural analysis.
    pub page_type: PageType,
}

/// A structural section of the page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageSection {
    /// HTML tag name of the section element (e.g. "article", "div", "section").
    pub tag: String,

    /// Detected role of this section.
    pub role: SectionRole,

    /// Total text length in characters within this section.
    pub text_length: u32,

    /// Ratio of link text to total text (0.0 to 1.0).
    pub link_density: f64,

    /// Heading text if a heading element is found within the section.
    pub heading: Option<String>,

    /// CSS selector that identifies this section.
    pub selector: String,
}

/// The semantic role of a page section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SectionRole {
    /// Primary content area (article body, main text).
    MainContent,
    /// Navigation links (menus, breadcrumbs).
    Navigation,
    /// Page header (logo, site title, top bar).
    Header,
    /// Page footer (copyright, footer links).
    Footer,
    /// Sidebar (related content, widgets).
    Sidebar,
    /// Advertisement block.
    Advertisement,
    /// User comments section.
    Comments,
    /// Related content or recommendations.
    RelatedContent,
    /// Could not be classified.
    Unknown,
}

/// The overall type of the page based on structural analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PageType {
    /// Long-form article or blog post.
    Article,
    /// E-commerce product detail page.
    ProductPage,
    /// Search engine results page.
    SearchResults,
    /// Category or listing page with multiple items.
    ListingPage,
    /// Login or authentication page.
    LoginPage,
    /// Page dominated by a form (contact, signup, etc.).
    FormPage,
    /// Marketing landing page.
    LandingPage,
    /// Could not determine page type.
    Unknown,
}

/// A navigation link element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavElement {
    /// Link text content.
    pub text: String,

    /// Link URL.
    pub href: String,

    /// Whether this link appears to be the currently active page.
    pub is_active: bool,
}
