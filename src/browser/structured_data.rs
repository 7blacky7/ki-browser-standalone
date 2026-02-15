//! Structured data extraction from web pages.
//!
//! This module provides extraction of structured metadata from web pages,
//! including JSON-LD, OpenGraph, Twitter Card, standard meta tags, and
//! Schema.org microdata. The extractor generates JavaScript that runs in
//! the browser context via `evaluate_script()` and returns a JSON string
//! that deserializes into [`StructuredPageData`].
//!
//! # Example
//!
//! ```rust,ignore
//! use ki_browser::browser::structured_data::StructuredDataExtractor;
//!
//! // Generate the extraction script
//! let script = StructuredDataExtractor::extraction_script();
//!
//! // Execute in browser via evaluate_script() and parse the result
//! let json_string = engine.evaluate_script(&script).await?;
//! let data: StructuredPageData = serde_json::from_str(&json_string)?;
//! ```

use serde::{Deserialize, Serialize};

/// Structured data extracted from a web page.
///
/// Contains all machine-readable metadata found on the page,
/// aggregated from multiple sources (JSON-LD, OpenGraph, meta tags, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredPageData {
    /// JSON-LD data from `<script type="application/ld+json">` tags.
    pub json_ld: Vec<serde_json::Value>,

    /// OpenGraph meta tags (`og:*` properties).
    pub opengraph: OpenGraphData,

    /// Twitter Card meta tags (`twitter:*` names).
    pub twitter_card: TwitterCardData,

    /// Standard meta tags and link elements.
    pub meta: MetaData,

    /// Microdata items (Schema.org `itemscope`/`itemtype`/`itemprop`).
    pub microdata: Vec<MicrodataItem>,
}

/// OpenGraph protocol metadata.
///
/// Represents data from `<meta property="og:*">` tags commonly used
/// by social media platforms for rich link previews.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenGraphData {
    /// Page title (`og:title`).
    pub title: Option<String>,

    /// Page description (`og:description`).
    pub description: Option<String>,

    /// Preview image URL (`og:image`).
    pub image: Option<String>,

    /// Canonical URL (`og:url`).
    pub url: Option<String>,

    /// Site name (`og:site_name`).
    pub site_name: Option<String>,

    /// Content type (`og:type`), e.g. "article", "website".
    pub og_type: Option<String>,

    /// Locale (`og:locale`), e.g. "en_US".
    pub locale: Option<String>,
}

/// Twitter Card metadata.
///
/// Represents data from `<meta name="twitter:*">` tags used for
/// Twitter/X link card previews.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TwitterCardData {
    /// Card type (`twitter:card`), e.g. "summary", "summary_large_image".
    pub card_type: Option<String>,

    /// Card title (`twitter:title`).
    pub title: Option<String>,

    /// Card description (`twitter:description`).
    pub description: Option<String>,

    /// Card image URL (`twitter:image`).
    pub image: Option<String>,

    /// Site Twitter handle (`twitter:site`), e.g. "@example".
    pub site: Option<String>,

    /// Content creator handle (`twitter:creator`).
    pub creator: Option<String>,
}

/// Standard HTML meta tags and link elements.
///
/// Aggregates common metadata that browsers, search engines,
/// and other tools use to understand the page.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetaData {
    /// Page title from `<title>` element.
    pub title: Option<String>,

    /// Meta description (`<meta name="description">`).
    pub description: Option<String>,

    /// Keywords from `<meta name="keywords">`, split by comma.
    pub keywords: Vec<String>,

    /// Canonical URL from `<link rel="canonical">`.
    pub canonical_url: Option<String>,

    /// Robots directive (`<meta name="robots">`).
    pub robots: Option<String>,

    /// Author (`<meta name="author">`).
    pub author: Option<String>,

    /// Page language from `<html lang="...">`.
    pub language: Option<String>,

    /// Character set from `<meta charset="...">`.
    pub charset: Option<String>,

    /// Viewport configuration (`<meta name="viewport">`).
    pub viewport: Option<String>,

    /// Favicon URL from `<link rel="icon">` or `<link rel="shortcut icon">`.
    pub favicon: Option<String>,

    /// Alternate URLs from `<link rel="alternate">` (e.g. hreflang, media).
    pub alternate_urls: Vec<AlternateUrl>,
}

/// An alternate URL for the page (e.g. different language or media version).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternateUrl {
    /// The URL of the alternate version.
    pub href: String,

    /// Language code (from `hreflang` attribute), e.g. "de", "en-US".
    pub hreflang: Option<String>,

    /// Media query (from `media` attribute), e.g. "handheld".
    pub media: Option<String>,
}

/// A Schema.org microdata item extracted from `itemscope`/`itemtype` elements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrodataItem {
    /// The Schema.org type URL (from `itemtype` attribute).
    pub item_type: String,

    /// Properties extracted from nested `itemprop` elements.
    pub properties: serde_json::Map<String, serde_json::Value>,
}

/// Extractor that generates JavaScript to extract structured data from a page.
///
/// The generated script is designed to be executed via `evaluate_script()` in
/// the browser context. It returns a JSON string that can be deserialized
/// into [`StructuredPageData`].
pub struct StructuredDataExtractor;

impl StructuredDataExtractor {
    /// Generate JavaScript that extracts all structured data from the current page.
    ///
    /// The returned script, when evaluated in a browser context, produces a
    /// JSON string containing all structured metadata found on the page.
    ///
    /// # Extraction sources
    ///
    /// 1. `<script type="application/ld+json">` tags (JSON-LD)
    /// 2. `<meta property="og:*">` tags (OpenGraph)
    /// 3. `<meta name="twitter:*">` tags (Twitter Card)
    /// 4. Standard `<meta>` tags (description, keywords, author, robots, etc.)
    /// 5. `<link rel="canonical">` (canonical URL)
    /// 6. `<link rel="alternate">` (alternate URLs)
    /// 7. `<link rel="icon">` / `<link rel="shortcut icon">` (favicon)
    /// 8. Elements with `itemscope`/`itemtype`/`itemprop` (Schema.org microdata)
    ///
    /// # Returns
    ///
    /// A JavaScript string that evaluates to a JSON string of [`StructuredPageData`].
    pub fn extraction_script() -> String {
        r#"
(function() {
    'use strict';

    var result = {
        json_ld: [],
        opengraph: {},
        twitter_card: {},
        meta: {
            keywords: [],
            alternate_urls: []
        },
        microdata: []
    };

    // 1. Extract JSON-LD from <script type="application/ld+json"> tags
    var ldScripts = document.querySelectorAll('script[type="application/ld+json"]');
    for (var i = 0; i < ldScripts.length; i++) {
        try {
            var content = ldScripts[i].textContent.trim();
            if (content) {
                var parsed = JSON.parse(content);
                result.json_ld.push(parsed);
            }
        } catch (e) {
            // Skip malformed JSON-LD blocks
        }
    }

    // 2. Extract OpenGraph meta tags (<meta property="og:*">)
    var ogMap = {
        'og:title': 'title',
        'og:description': 'description',
        'og:image': 'image',
        'og:url': 'url',
        'og:site_name': 'site_name',
        'og:type': 'og_type',
        'og:locale': 'locale'
    };
    var ogMetas = document.querySelectorAll('meta[property^="og:"]');
    for (var i = 0; i < ogMetas.length; i++) {
        var prop = ogMetas[i].getAttribute('property');
        var val = ogMetas[i].getAttribute('content');
        if (prop && val && ogMap[prop]) {
            result.opengraph[ogMap[prop]] = val;
        }
    }

    // 3. Extract Twitter Card meta tags (<meta name="twitter:*">)
    var twMap = {
        'twitter:card': 'card_type',
        'twitter:title': 'title',
        'twitter:description': 'description',
        'twitter:image': 'image',
        'twitter:site': 'site',
        'twitter:creator': 'creator'
    };
    var twMetas = document.querySelectorAll('meta[name^="twitter:"]');
    for (var i = 0; i < twMetas.length; i++) {
        var name = twMetas[i].getAttribute('name');
        var val = twMetas[i].getAttribute('content');
        if (name && val && twMap[name]) {
            result.twitter_card[twMap[name]] = val;
        }
    }

    // 4. Extract standard meta tags
    // Title from <title> element
    var titleEl = document.querySelector('title');
    if (titleEl && titleEl.textContent) {
        result.meta.title = titleEl.textContent.trim();
    }

    // Description
    var descMeta = document.querySelector('meta[name="description"]');
    if (descMeta) {
        result.meta.description = descMeta.getAttribute('content') || null;
    }

    // Keywords
    var kwMeta = document.querySelector('meta[name="keywords"]');
    if (kwMeta) {
        var kwContent = kwMeta.getAttribute('content');
        if (kwContent) {
            result.meta.keywords = kwContent.split(',').map(function(k) {
                return k.trim();
            }).filter(function(k) {
                return k.length > 0;
            });
        }
    }

    // Robots
    var robotsMeta = document.querySelector('meta[name="robots"]');
    if (robotsMeta) {
        result.meta.robots = robotsMeta.getAttribute('content') || null;
    }

    // Author
    var authorMeta = document.querySelector('meta[name="author"]');
    if (authorMeta) {
        result.meta.author = authorMeta.getAttribute('content') || null;
    }

    // Language from <html lang="...">
    var htmlLang = document.documentElement.getAttribute('lang');
    if (htmlLang) {
        result.meta.language = htmlLang;
    }

    // Charset from <meta charset="..."> or <meta http-equiv="Content-Type">
    var charsetMeta = document.querySelector('meta[charset]');
    if (charsetMeta) {
        result.meta.charset = charsetMeta.getAttribute('charset') || null;
    } else {
        var ctMeta = document.querySelector('meta[http-equiv="Content-Type"]');
        if (ctMeta) {
            var ctContent = ctMeta.getAttribute('content') || '';
            var charsetMatch = ctContent.match(/charset=([^\s;]+)/i);
            if (charsetMatch) {
                result.meta.charset = charsetMatch[1];
            }
        }
    }

    // Viewport
    var viewportMeta = document.querySelector('meta[name="viewport"]');
    if (viewportMeta) {
        result.meta.viewport = viewportMeta.getAttribute('content') || null;
    }

    // 5. Canonical URL from <link rel="canonical">
    var canonicalLink = document.querySelector('link[rel="canonical"]');
    if (canonicalLink) {
        result.meta.canonical_url = canonicalLink.getAttribute('href') || null;
    }

    // 6. Alternate URLs from <link rel="alternate">
    var altLinks = document.querySelectorAll('link[rel="alternate"]');
    for (var i = 0; i < altLinks.length; i++) {
        var href = altLinks[i].getAttribute('href');
        if (href) {
            result.meta.alternate_urls.push({
                href: href,
                hreflang: altLinks[i].getAttribute('hreflang') || null,
                media: altLinks[i].getAttribute('media') || null
            });
        }
    }

    // 7. Favicon from <link rel="icon"> or <link rel="shortcut icon">
    var faviconLink = document.querySelector('link[rel="icon"], link[rel="shortcut icon"]');
    if (faviconLink) {
        result.meta.favicon = faviconLink.getAttribute('href') || null;
    }

    // 8. Extract microdata from itemscope/itemtype/itemprop attributes
    var itemElements = document.querySelectorAll('[itemscope][itemtype]');
    for (var i = 0; i < itemElements.length; i++) {
        var itemEl = itemElements[i];
        var itemType = itemEl.getAttribute('itemtype') || '';
        var properties = {};

        var propElements = itemEl.querySelectorAll('[itemprop]');
        for (var j = 0; j < propElements.length; j++) {
            var propEl = propElements[j];
            // Skip if this itemprop belongs to a nested itemscope
            var parent = propEl.parentElement;
            var belongsToNested = false;
            while (parent && parent !== itemEl) {
                if (parent.hasAttribute('itemscope')) {
                    belongsToNested = true;
                    break;
                }
                parent = parent.parentElement;
            }
            if (belongsToNested) continue;

            var propName = propEl.getAttribute('itemprop');
            if (!propName) continue;

            var propValue;
            var tag = propEl.tagName.toLowerCase();
            if (tag === 'meta') {
                propValue = propEl.getAttribute('content') || '';
            } else if (tag === 'a' || tag === 'link') {
                propValue = propEl.getAttribute('href') || '';
            } else if (tag === 'img') {
                propValue = propEl.getAttribute('src') || '';
            } else if (tag === 'time') {
                propValue = propEl.getAttribute('datetime') || propEl.textContent.trim();
            } else if (tag === 'data' || tag === 'meter') {
                propValue = propEl.getAttribute('value') || propEl.textContent.trim();
            } else {
                propValue = propEl.textContent.trim();
            }

            // Handle multiple values for the same property
            if (properties[propName] !== undefined) {
                if (Array.isArray(properties[propName])) {
                    properties[propName].push(propValue);
                } else {
                    properties[propName] = [properties[propName], propValue];
                }
            } else {
                properties[propName] = propValue;
            }
        }

        result.microdata.push({
            item_type: itemType,
            properties: properties
        });
    }

    return JSON.stringify(result);
})()
"#
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extraction_script_is_nonempty() {
        let script = StructuredDataExtractor::extraction_script();
        assert!(!script.is_empty());
    }

    #[test]
    fn test_extraction_script_is_iife() {
        let script = StructuredDataExtractor::extraction_script();
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
    fn test_extraction_script_contains_json_ld_extraction() {
        let script = StructuredDataExtractor::extraction_script();
        assert!(script.contains("application/ld+json"));
        assert!(script.contains("JSON.parse"));
    }

    #[test]
    fn test_extraction_script_contains_opengraph_extraction() {
        let script = StructuredDataExtractor::extraction_script();
        assert!(script.contains("og:title"));
        assert!(script.contains("og:description"));
        assert!(script.contains("og:image"));
        assert!(script.contains("og:url"));
        assert!(script.contains("og:site_name"));
        assert!(script.contains("og:type"));
        assert!(script.contains("og:locale"));
    }

    #[test]
    fn test_extraction_script_contains_twitter_card_extraction() {
        let script = StructuredDataExtractor::extraction_script();
        assert!(script.contains("twitter:card"));
        assert!(script.contains("twitter:title"));
        assert!(script.contains("twitter:description"));
        assert!(script.contains("twitter:image"));
        assert!(script.contains("twitter:site"));
        assert!(script.contains("twitter:creator"));
    }

    #[test]
    fn test_extraction_script_contains_meta_extraction() {
        let script = StructuredDataExtractor::extraction_script();
        assert!(script.contains(r#"meta[name="description"]"#));
        assert!(script.contains(r#"meta[name="keywords"]"#));
        assert!(script.contains(r#"meta[name="robots"]"#));
        assert!(script.contains(r#"meta[name="author"]"#));
        assert!(script.contains(r#"meta[name="viewport"]"#));
        assert!(script.contains(r#"meta[charset]"#));
    }

    #[test]
    fn test_extraction_script_contains_link_extraction() {
        let script = StructuredDataExtractor::extraction_script();
        assert!(script.contains(r#"link[rel="canonical"]"#));
        assert!(script.contains(r#"link[rel="alternate"]"#));
        assert!(script.contains(r#"link[rel="icon"]"#));
    }

    #[test]
    fn test_extraction_script_contains_microdata_extraction() {
        let script = StructuredDataExtractor::extraction_script();
        assert!(script.contains("itemscope"));
        assert!(script.contains("itemtype"));
        assert!(script.contains("itemprop"));
    }

    #[test]
    fn test_extraction_script_returns_json() {
        let script = StructuredDataExtractor::extraction_script();
        assert!(script.contains("JSON.stringify(result)"));
    }

    #[test]
    fn test_structured_page_data_serde() {
        let data = StructuredPageData {
            json_ld: vec![serde_json::json!({"@type": "Article", "name": "Test"})],
            opengraph: OpenGraphData {
                title: Some("OG Title".to_string()),
                description: Some("OG Desc".to_string()),
                ..Default::default()
            },
            twitter_card: TwitterCardData {
                card_type: Some("summary".to_string()),
                ..Default::default()
            },
            meta: MetaData {
                title: Some("Page Title".to_string()),
                keywords: vec!["rust".to_string(), "browser".to_string()],
                ..Default::default()
            },
            microdata: vec![MicrodataItem {
                item_type: "https://schema.org/Product".to_string(),
                properties: {
                    let mut m = serde_json::Map::new();
                    m.insert("name".to_string(), serde_json::Value::String("Widget".to_string()));
                    m
                },
            }],
        };

        let json = serde_json::to_string(&data).unwrap();
        let parsed: StructuredPageData = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.json_ld.len(), 1);
        assert_eq!(parsed.opengraph.title, Some("OG Title".to_string()));
        assert_eq!(parsed.twitter_card.card_type, Some("summary".to_string()));
        assert_eq!(parsed.meta.title, Some("Page Title".to_string()));
        assert_eq!(parsed.meta.keywords.len(), 2);
        assert_eq!(parsed.microdata.len(), 1);
        assert_eq!(
            parsed.microdata[0].item_type,
            "https://schema.org/Product"
        );
    }

    #[test]
    fn test_opengraph_data_default() {
        let og = OpenGraphData::default();
        assert!(og.title.is_none());
        assert!(og.description.is_none());
        assert!(og.image.is_none());
        assert!(og.url.is_none());
        assert!(og.site_name.is_none());
        assert!(og.og_type.is_none());
        assert!(og.locale.is_none());
    }

    #[test]
    fn test_twitter_card_data_default() {
        let tc = TwitterCardData::default();
        assert!(tc.card_type.is_none());
        assert!(tc.title.is_none());
        assert!(tc.description.is_none());
        assert!(tc.image.is_none());
        assert!(tc.site.is_none());
        assert!(tc.creator.is_none());
    }

    #[test]
    fn test_meta_data_default() {
        let meta = MetaData::default();
        assert!(meta.title.is_none());
        assert!(meta.description.is_none());
        assert!(meta.keywords.is_empty());
        assert!(meta.canonical_url.is_none());
        assert!(meta.robots.is_none());
        assert!(meta.author.is_none());
        assert!(meta.language.is_none());
        assert!(meta.charset.is_none());
        assert!(meta.viewport.is_none());
        assert!(meta.favicon.is_none());
        assert!(meta.alternate_urls.is_empty());
    }

    #[test]
    fn test_alternate_url_serde() {
        let alt = AlternateUrl {
            href: "https://example.com/de".to_string(),
            hreflang: Some("de".to_string()),
            media: None,
        };

        let json = serde_json::to_string(&alt).unwrap();
        let parsed: AlternateUrl = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.href, "https://example.com/de");
        assert_eq!(parsed.hreflang, Some("de".to_string()));
        assert!(parsed.media.is_none());
    }

    #[test]
    fn test_microdata_item_serde() {
        let item = MicrodataItem {
            item_type: "https://schema.org/Person".to_string(),
            properties: {
                let mut m = serde_json::Map::new();
                m.insert(
                    "name".to_string(),
                    serde_json::Value::String("John Doe".to_string()),
                );
                m.insert(
                    "jobTitle".to_string(),
                    serde_json::Value::String("Developer".to_string()),
                );
                m
            },
        };

        let json = serde_json::to_string(&item).unwrap();
        let parsed: MicrodataItem = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.item_type, "https://schema.org/Person");
        assert_eq!(parsed.properties.len(), 2);
    }

    #[test]
    fn test_extraction_script_handles_malformed_json_ld() {
        let script = StructuredDataExtractor::extraction_script();
        // Script should have try/catch around JSON.parse for robustness
        assert!(script.contains("try"));
        assert!(script.contains("catch"));
    }

    #[test]
    fn test_extraction_script_microdata_skips_nested_scopes() {
        let script = StructuredDataExtractor::extraction_script();
        // Script should check for nested itemscope to avoid double-counting
        assert!(script.contains("belongsToNested"));
        assert!(script.contains("itemscope"));
    }
}
