//! Intelligent content extraction and page structure analysis.
//!
//! This module provides Readability-like content extraction and page structure
//! analysis. It generates JavaScript that runs in the browser context via
//! `evaluate_script()` and returns JSON strings that deserialize into
//! [`ExtractedContent`] and [`PageStructure`].
//!
//! # Algorithms
//!
//! - **Text density scoring**: Measures text length relative to tag count
//!   to identify content-rich areas vs. boilerplate.
//! - **Link density**: Calculates the ratio of link text to total text
//!   to filter out navigation-heavy sections.
//! - **Semantic tag recognition**: Leverages HTML5 semantic elements
//!   (`<article>`, `<main>`, `<nav>`, `<aside>`, `<header>`, `<footer>`)
//!   and ARIA roles for structural classification.
//!
//! # Example
//!
//! ```rust,ignore
//! use ki_browser_standalone::browser::content_extractor::ContentExtractor;
//!
//! // Extract main content
//! let script = ContentExtractor::content_extraction_script();
//! let json_string = engine.evaluate_script(&script).await?;
//! let content: ExtractedContent = serde_json::from_str(&json_string)?;
//!
//! // Analyze page structure
//! let script = ContentExtractor::structure_analysis_script();
//! let json_string = engine.evaluate_script(&script).await?;
//! let structure: PageStructure = serde_json::from_str(&json_string)?;
//! ```

mod extractor;
mod types;

pub use extractor::ContentExtractor;
pub use types::{
    ExtractedContent, NavElement, PageSection, PageStructure, PageType, SectionRole,
};

#[cfg(test)]
mod tests {
    use super::*;

    // --- Content extraction script tests ---

    #[test]
    fn test_content_extraction_script_is_nonempty() {
        let script = ContentExtractor::content_extraction_script();
        assert!(!script.is_empty());
    }

    #[test]
    fn test_content_extraction_script_is_iife() {
        let script = ContentExtractor::content_extraction_script();
        let trimmed = script.trim();
        assert!(
            trimmed.starts_with("(function()"),
            "Script should be an IIFE"
        );
        assert!(
            trimmed.ends_with("()"),
            "Script should end with invocation parentheses"
        );
    }

    #[test]
    fn test_content_extraction_script_returns_json() {
        let script = ContentExtractor::content_extraction_script();
        assert!(script.contains("JSON.stringify(result)"));
    }

    #[test]
    fn test_content_extraction_script_has_text_density_algorithm() {
        let script = ContentExtractor::content_extraction_script();
        assert!(script.contains("textDensity"));
        assert!(script.contains("getTextLength"));
        assert!(script.contains("getLinkDensity"));
    }

    #[test]
    fn test_content_extraction_script_recognizes_semantic_tags() {
        let script = ContentExtractor::content_extraction_script();
        assert!(script.contains("article"));
        assert!(script.contains("main"));
        assert!(script.contains("nav"));
        assert!(script.contains("aside"));
        assert!(script.contains("header"));
        assert!(script.contains("footer"));
    }

    #[test]
    fn test_content_extraction_script_checks_aria_roles() {
        let script = ContentExtractor::content_extraction_script();
        assert!(script.contains(r#"role="main"#));
        assert!(script.contains(r#"role="navigation"#));
        assert!(script.contains(r#"role="banner"#));
    }

    #[test]
    fn test_content_extraction_script_filters_boilerplate() {
        let script = ContentExtractor::content_extraction_script();
        assert!(script.contains("isBoilerplate"));
        assert!(script.contains("comment"));
        assert!(script.contains("sidebar"));
        assert!(script.contains("advertis"));
    }

    #[test]
    fn test_content_extraction_script_detects_author() {
        let script = ContentExtractor::content_extraction_script();
        assert!(script.contains(r#"meta[name="author"]"#));
        assert!(script.contains("byline"));
        assert!(script.contains("itemprop=\"author\""));
    }

    #[test]
    fn test_content_extraction_script_detects_date() {
        let script = ContentExtractor::content_extraction_script();
        assert!(script.contains("article:published_time"));
        assert!(script.contains("datePublished"));
        assert!(script.contains("datetime"));
    }

    #[test]
    fn test_content_extraction_script_calculates_reading_time() {
        let script = ContentExtractor::content_extraction_script();
        assert!(script.contains("reading_time_minutes"));
        assert!(script.contains("word_count"));
        // 200 words per minute reading speed
        assert!(script.contains("200"));
    }

    #[test]
    fn test_content_extraction_script_scores_candidates() {
        let script = ContentExtractor::content_extraction_script();
        assert!(script.contains("candidates"));
        assert!(script.contains("score"));
        assert!(script.contains("sort"));
    }

    // --- Structure analysis script tests ---

    #[test]
    fn test_structure_analysis_script_is_nonempty() {
        let script = ContentExtractor::structure_analysis_script();
        assert!(!script.is_empty());
    }

    #[test]
    fn test_structure_analysis_script_is_iife() {
        let script = ContentExtractor::structure_analysis_script();
        let trimmed = script.trim();
        assert!(
            trimmed.starts_with("(function()"),
            "Script should be an IIFE"
        );
        assert!(
            trimmed.ends_with("()"),
            "Script should end with invocation parentheses"
        );
    }

    #[test]
    fn test_structure_analysis_script_returns_json() {
        let script = ContentExtractor::structure_analysis_script();
        assert!(script.contains("JSON.stringify(result)"));
    }

    #[test]
    fn test_structure_analysis_script_classifies_sections() {
        let script = ContentExtractor::structure_analysis_script();
        assert!(script.contains("MainContent"));
        assert!(script.contains("Navigation"));
        assert!(script.contains("Header"));
        assert!(script.contains("Footer"));
        assert!(script.contains("Sidebar"));
        assert!(script.contains("Advertisement"));
        assert!(script.contains("Comments"));
        assert!(script.contains("RelatedContent"));
        assert!(script.contains("Unknown"));
    }

    #[test]
    fn test_structure_analysis_script_detects_page_types() {
        let script = ContentExtractor::structure_analysis_script();
        assert!(script.contains("Article"));
        assert!(script.contains("ProductPage"));
        assert!(script.contains("SearchResults"));
        assert!(script.contains("ListingPage"));
        assert!(script.contains("LoginPage"));
        assert!(script.contains("FormPage"));
        assert!(script.contains("LandingPage"));
    }

    #[test]
    fn test_structure_analysis_script_extracts_navigation() {
        let script = ContentExtractor::structure_analysis_script();
        assert!(script.contains("nav a[href]"));
        assert!(script.contains("role=\"navigation\""));
        assert!(script.contains("is_active"));
        assert!(script.contains("aria-current"));
    }

    #[test]
    fn test_structure_analysis_script_checks_aria_roles() {
        let script = ContentExtractor::structure_analysis_script();
        assert!(script.contains("role === 'main'"));
        assert!(script.contains("role === 'navigation'"));
        assert!(script.contains("role === 'banner'"));
        assert!(script.contains("role === 'contentinfo'"));
        assert!(script.contains("role === 'complementary'"));
    }

    #[test]
    fn test_structure_analysis_script_calculates_link_density() {
        let script = ContentExtractor::structure_analysis_script();
        assert!(script.contains("getLinkDensity"));
        assert!(script.contains("link_density"));
    }

    #[test]
    fn test_structure_analysis_script_falls_back_to_divs() {
        let script = ContentExtractor::structure_analysis_script();
        // When no semantic sections found, should analyze top-level divs
        assert!(script.contains("body > div"));
    }

    // --- Struct serialization tests ---

    #[test]
    fn test_extracted_content_serde() {
        let content = ExtractedContent {
            content: "This is the main article text.".to_string(),
            content_html: "<p>This is the main article text.</p>".to_string(),
            title: "Test Article".to_string(),
            author: Some("Jane Doe".to_string()),
            published_date: Some("2025-01-15T10:00:00Z".to_string()),
            reading_time_minutes: 3,
            word_count: 600,
            language: Some("en".to_string()),
            score: 0.85,
        };

        let json = serde_json::to_string(&content).unwrap();
        let parsed: ExtractedContent = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.title, "Test Article");
        assert_eq!(parsed.author, Some("Jane Doe".to_string()));
        assert_eq!(parsed.reading_time_minutes, 3);
        assert_eq!(parsed.word_count, 600);
        assert_eq!(parsed.score, 0.85);
    }

    #[test]
    fn test_extracted_content_minimal() {
        let content = ExtractedContent {
            content: String::new(),
            content_html: String::new(),
            title: String::new(),
            author: None,
            published_date: None,
            reading_time_minutes: 0,
            word_count: 0,
            language: None,
            score: 0.0,
        };

        let json = serde_json::to_string(&content).unwrap();
        let parsed: ExtractedContent = serde_json::from_str(&json).unwrap();
        assert!(parsed.author.is_none());
        assert!(parsed.published_date.is_none());
        assert!(parsed.language.is_none());
    }

    #[test]
    fn test_page_structure_serde() {
        let structure = PageStructure {
            sections: vec![
                PageSection {
                    tag: "article".to_string(),
                    role: SectionRole::MainContent,
                    text_length: 5000,
                    link_density: 0.05,
                    heading: Some("Breaking News".to_string()),
                    selector: "article.main-story".to_string(),
                },
                PageSection {
                    tag: "nav".to_string(),
                    role: SectionRole::Navigation,
                    text_length: 200,
                    link_density: 0.95,
                    heading: None,
                    selector: "nav.main-nav".to_string(),
                },
            ],
            navigation: vec![NavElement {
                text: "Home".to_string(),
                href: "/".to_string(),
                is_active: true,
            }],
            page_type: PageType::Article,
        };

        let json = serde_json::to_string(&structure).unwrap();
        let parsed: PageStructure = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.sections.len(), 2);
        assert_eq!(parsed.navigation.len(), 1);
        assert!(parsed.navigation[0].is_active);
    }

    #[test]
    fn test_section_role_serde_roundtrip() {
        let roles = vec![
            SectionRole::MainContent,
            SectionRole::Navigation,
            SectionRole::Header,
            SectionRole::Footer,
            SectionRole::Sidebar,
            SectionRole::Advertisement,
            SectionRole::Comments,
            SectionRole::RelatedContent,
            SectionRole::Unknown,
        ];

        for role in roles {
            let json = serde_json::to_string(&role).unwrap();
            let parsed: SectionRole = serde_json::from_str(&json).unwrap();
            // Verify roundtrip by re-serializing
            assert_eq!(json, serde_json::to_string(&parsed).unwrap());
        }
    }

    #[test]
    fn test_page_type_serde_roundtrip() {
        let types = vec![
            PageType::Article,
            PageType::ProductPage,
            PageType::SearchResults,
            PageType::ListingPage,
            PageType::LoginPage,
            PageType::FormPage,
            PageType::LandingPage,
            PageType::Unknown,
        ];

        for pt in types {
            let json = serde_json::to_string(&pt).unwrap();
            let parsed: PageType = serde_json::from_str(&json).unwrap();
            assert_eq!(json, serde_json::to_string(&parsed).unwrap());
        }
    }

    #[test]
    fn test_nav_element_serde() {
        let nav = NavElement {
            text: "About Us".to_string(),
            href: "/about".to_string(),
            is_active: false,
        };

        let json = serde_json::to_string(&nav).unwrap();
        let parsed: NavElement = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.text, "About Us");
        assert_eq!(parsed.href, "/about");
        assert!(!parsed.is_active);
    }

    #[test]
    fn test_page_section_serde() {
        let section = PageSection {
            tag: "aside".to_string(),
            role: SectionRole::Sidebar,
            text_length: 300,
            link_density: 0.4,
            heading: Some("Related Articles".to_string()),
            selector: "aside.sidebar".to_string(),
        };

        let json = serde_json::to_string(&section).unwrap();
        let parsed: PageSection = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tag, "aside");
        assert_eq!(parsed.text_length, 300);
        assert_eq!(parsed.link_density, 0.4);
        assert_eq!(parsed.heading, Some("Related Articles".to_string()));
    }
}
