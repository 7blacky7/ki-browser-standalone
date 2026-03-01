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

/// Content extractor that generates JavaScript for browser-side execution.
///
/// Produces two independent scripts:
/// - [`content_extraction_script`](Self::content_extraction_script): Readability-like main content extraction
/// - [`structure_analysis_script`](Self::structure_analysis_script): Full page structure analysis
pub struct ContentExtractor;

impl ContentExtractor {
    /// Generate JavaScript for Readability-like content extraction.
    ///
    /// The script implements a text-density algorithm that:
    /// 1. Identifies candidate content containers (`<article>`, `<main>`, semantic blocks)
    /// 2. Scores each candidate by text density and link density
    /// 3. Selects the highest-scoring candidate as the main content
    /// 4. Extracts clean text and HTML from the winning candidate
    /// 5. Detects metadata (author, date, language, word count, reading time)
    ///
    /// # Returns
    ///
    /// A JavaScript string that evaluates to a JSON string of [`ExtractedContent`].
    pub fn content_extraction_script() -> String {
        r#"
(function() {
    'use strict';

    // --- Helper functions ---

    // Get visible text length of an element (excluding hidden children)
    function getTextLength(el) {
        var text = el.textContent || '';
        return text.trim().length;
    }

    // Count direct text characters (not from child elements' deep text)
    function getDirectTextLength(el) {
        var len = 0;
        for (var i = 0; i < el.childNodes.length; i++) {
            if (el.childNodes[i].nodeType === 3) {
                len += el.childNodes[i].textContent.trim().length;
            }
        }
        return len;
    }

    // Calculate link density: ratio of link text to total text
    function getLinkDensity(el) {
        var textLen = getTextLength(el);
        if (textLen === 0) return 0;
        var links = el.querySelectorAll('a');
        var linkTextLen = 0;
        for (var i = 0; i < links.length; i++) {
            linkTextLen += getTextLength(links[i]);
        }
        return linkTextLen / textLen;
    }

    // Count paragraph-like elements inside a container
    function countParagraphs(el) {
        return el.querySelectorAll('p, li, blockquote, pre, td').length;
    }

    // Count all element nodes inside a container
    function countTags(el) {
        return el.getElementsByTagName('*').length;
    }

    // Check if an element is likely boilerplate by class/id patterns
    function isBoilerplate(el) {
        var id = (el.id || '').toLowerCase();
        var cls = (el.className || '').toString().toLowerCase();
        var combined = id + ' ' + cls;
        var boilerplatePatterns = [
            'comment', 'footer', 'footnote', 'sidebar', 'widget',
            'banner', 'advertis', 'ad-', 'ad_', 'social', 'share',
            'related', 'popular', 'recommend', 'outbrain', 'taboola',
            'newsletter', 'subscribe', 'signup', 'sign-up', 'cookie',
            'popup', 'modal', 'overlay', 'menu', 'breadcrumb',
            'pagination', 'pager', 'nav'
        ];
        for (var i = 0; i < boilerplatePatterns.length; i++) {
            if (combined.indexOf(boilerplatePatterns[i]) !== -1) return true;
        }
        return false;
    }

    // Check if element is likely content by class/id patterns
    function isContentHint(el) {
        var id = (el.id || '').toLowerCase();
        var cls = (el.className || '').toString().toLowerCase();
        var combined = id + ' ' + cls;
        var contentPatterns = [
            'article', 'content', 'entry', 'post', 'text', 'body',
            'story', 'main', 'blog', 'page-content', 'article-body'
        ];
        for (var i = 0; i < contentPatterns.length; i++) {
            if (combined.indexOf(contentPatterns[i]) !== -1) return true;
        }
        return false;
    }

    // Build a simple CSS selector for an element
    function buildSelector(el) {
        if (el.id) return '#' + CSS.escape(el.id);
        var tag = el.tagName.toLowerCase();
        var classes = Array.from(el.classList || []).slice(0, 2)
            .map(function(c) { return '.' + CSS.escape(c); }).join('');
        return tag + classes;
    }

    // Extract clean text from an element
    function getCleanText(el) {
        var clone = el.cloneNode(true);
        // Remove script, style, nav, aside, footer elements
        var removeSelectors = 'script, style, nav, aside, footer, header, ' +
            'form, iframe, [role="navigation"], [role="banner"], ' +
            '[role="complementary"], [role="contentinfo"], .ad, .ads, ' +
            '.advertisement, .social, .share, .comments';
        var toRemove = clone.querySelectorAll(removeSelectors);
        for (var i = toRemove.length - 1; i >= 0; i--) {
            toRemove[i].parentNode.removeChild(toRemove[i]);
        }
        return clone.textContent.trim().replace(/\s+/g, ' ');
    }

    // Extract clean HTML from an element
    function getCleanHtml(el) {
        var clone = el.cloneNode(true);
        var removeSelectors = 'script, style, nav, aside, footer, header, ' +
            'form, iframe, [role="navigation"], [role="banner"], ' +
            '[role="complementary"], [role="contentinfo"], .ad, .ads, ' +
            '.advertisement, .social, .share, .comments';
        var toRemove = clone.querySelectorAll(removeSelectors);
        for (var i = toRemove.length - 1; i >= 0; i--) {
            toRemove[i].parentNode.removeChild(toRemove[i]);
        }
        return clone.innerHTML.trim();
    }

    // --- Candidate scoring ---

    // Find and score content candidates
    var candidates = [];
    var containers = document.querySelectorAll(
        'article, main, [role="main"], [role="article"], ' +
        'div, section, td'
    );

    for (var i = 0; i < containers.length; i++) {
        var el = containers[i];
        var textLen = getTextLength(el);
        if (textLen < 100) continue; // Skip very short elements

        var tagCount = countTags(el);
        if (tagCount === 0) tagCount = 1;
        var paragraphs = countParagraphs(el);
        var linkDensity = getLinkDensity(el);

        // Text density: text length per tag
        var textDensity = textLen / tagCount;

        // Base score from text density and paragraph count
        var score = textDensity * 0.5 + paragraphs * 10;

        // Bonus for semantic content tags
        var tag = el.tagName.toLowerCase();
        if (tag === 'article') score *= 2.0;
        else if (tag === 'main') score *= 1.8;
        else if (el.getAttribute('role') === 'main') score *= 1.8;
        else if (el.getAttribute('role') === 'article') score *= 2.0;

        // Bonus for content-hinting class/id names
        if (isContentHint(el)) score *= 1.5;

        // Penalty for high link density (likely navigation)
        if (linkDensity > 0.5) score *= 0.3;
        else if (linkDensity > 0.3) score *= 0.6;

        // Penalty for boilerplate signals
        if (isBoilerplate(el)) score *= 0.2;

        // Penalty for very deeply nested elements (likely wrappers)
        var depth = 0;
        var p = el.parentElement;
        while (p) { depth++; p = p.parentElement; }
        if (depth > 15) score *= 0.5;

        candidates.push({
            element: el,
            score: score,
            textLen: textLen,
            linkDensity: linkDensity
        });
    }

    // Sort by score descending
    candidates.sort(function(a, b) { return b.score - a.score; });

    // Select best candidate
    var bestCandidate = candidates.length > 0 ? candidates[0] : null;

    // Fallback to body if no good candidate found
    var contentEl = bestCandidate ? bestCandidate.element : document.body;
    var finalScore = bestCandidate ? bestCandidate.score : 0;

    // Normalize score to 0.0-1.0 range
    var maxPossibleScore = 5000;
    var normalizedScore = Math.min(finalScore / maxPossibleScore, 1.0);

    // --- Extract content ---
    var contentText = getCleanText(contentEl);
    var contentHtml = getCleanHtml(contentEl);

    // --- Extract metadata ---

    // Title: prefer <title>, fall back to first <h1>
    var title = '';
    var titleEl = document.querySelector('title');
    if (titleEl) {
        title = titleEl.textContent.trim();
    }
    if (!title) {
        var h1 = document.querySelector('h1');
        if (h1) title = h1.textContent.trim();
    }

    // Author detection
    var author = null;
    // Try meta tags first
    var authorMeta = document.querySelector(
        'meta[name="author"], meta[property="article:author"], ' +
        'meta[name="dc.creator"], meta[name="DC.creator"]'
    );
    if (authorMeta) {
        author = authorMeta.getAttribute('content') || null;
    }
    // Try JSON-LD
    if (!author) {
        var ldScripts = document.querySelectorAll('script[type="application/ld+json"]');
        for (var i = 0; i < ldScripts.length; i++) {
            try {
                var ld = JSON.parse(ldScripts[i].textContent);
                if (ld.author) {
                    if (typeof ld.author === 'string') { author = ld.author; break; }
                    if (ld.author.name) { author = ld.author.name; break; }
                }
            } catch(e) {}
        }
    }
    // Try byline patterns in content
    if (!author) {
        var bylineEl = document.querySelector(
            '[class*="byline"], [class*="author"], [rel="author"], ' +
            '[itemprop="author"], .post-author, .entry-author'
        );
        if (bylineEl) {
            var bylineText = bylineEl.textContent.trim();
            if (bylineText.length > 0 && bylineText.length < 100) {
                author = bylineText.replace(/^by\s+/i, '').trim();
            }
        }
    }

    // Published date detection
    var publishedDate = null;
    // Try meta tags
    var dateMeta = document.querySelector(
        'meta[property="article:published_time"], ' +
        'meta[name="date"], meta[name="DC.date"], ' +
        'meta[name="dc.date"], meta[name="publishdate"], ' +
        'meta[property="og:article:published_time"]'
    );
    if (dateMeta) {
        publishedDate = dateMeta.getAttribute('content') || null;
    }
    // Try time element
    if (!publishedDate) {
        var timeEl = document.querySelector(
            'time[datetime], time[pubdate], [itemprop="datePublished"]'
        );
        if (timeEl) {
            publishedDate = timeEl.getAttribute('datetime') ||
                            timeEl.getAttribute('content') ||
                            timeEl.textContent.trim() || null;
        }
    }
    // Try JSON-LD
    if (!publishedDate) {
        var ldScripts = document.querySelectorAll('script[type="application/ld+json"]');
        for (var i = 0; i < ldScripts.length; i++) {
            try {
                var ld = JSON.parse(ldScripts[i].textContent);
                if (ld.datePublished) { publishedDate = ld.datePublished; break; }
            } catch(e) {}
        }
    }

    // Word count and reading time
    var words = contentText.split(/\s+/).filter(function(w) { return w.length > 0; });
    var wordCount = words.length;
    var readingTimeMinutes = Math.max(1, Math.ceil(wordCount / 200));

    // Language
    var language = document.documentElement.getAttribute('lang') || null;

    var result = {
        content: contentText,
        content_html: contentHtml,
        title: title,
        author: author,
        published_date: publishedDate,
        reading_time_minutes: readingTimeMinutes,
        word_count: wordCount,
        language: language,
        score: Math.round(normalizedScore * 1000) / 1000
    };

    return JSON.stringify(result);
})()
"#
        .to_string()
    }

    /// Generate JavaScript for page structure analysis.
    ///
    /// The script analyzes the page layout by:
    /// 1. Identifying structural sections using semantic tags and ARIA roles
    /// 2. Classifying each section by its role (content, navigation, header, etc.)
    /// 3. Extracting navigation links from `<nav>` elements and navigation roles
    /// 4. Detecting the overall page type from structural signals
    ///
    /// # Returns
    ///
    /// A JavaScript string that evaluates to a JSON string of [`PageStructure`].
    pub fn structure_analysis_script() -> String {
        r#"
(function() {
    'use strict';

    // --- Helper functions ---

    function getTextLength(el) {
        return (el.textContent || '').trim().length;
    }

    function getLinkDensity(el) {
        var textLen = getTextLength(el);
        if (textLen === 0) return 0;
        var links = el.querySelectorAll('a');
        var linkTextLen = 0;
        for (var i = 0; i < links.length; i++) {
            linkTextLen += getTextLength(links[i]);
        }
        return Math.round((linkTextLen / textLen) * 1000) / 1000;
    }

    function buildSelector(el) {
        if (el.id) return '#' + CSS.escape(el.id);
        var tag = el.tagName.toLowerCase();
        var classes = Array.from(el.classList || []).slice(0, 2)
            .map(function(c) { return '.' + CSS.escape(c); }).join('');
        var parent = el.parentElement;
        if (parent) {
            var siblings = Array.from(parent.children).filter(function(c) {
                return c.tagName === el.tagName;
            });
            if (siblings.length > 1) {
                var idx = siblings.indexOf(el) + 1;
                return tag + classes + ':nth-of-type(' + idx + ')';
            }
        }
        return tag + classes;
    }

    function findHeading(el) {
        var heading = el.querySelector('h1, h2, h3, h4, h5, h6');
        if (heading) return heading.textContent.trim().substring(0, 200);
        return null;
    }

    // Classify a section element by its semantic role
    function classifySection(el) {
        var tag = el.tagName.toLowerCase();
        var role = (el.getAttribute('role') || '').toLowerCase();
        var id = (el.id || '').toLowerCase();
        var cls = (el.className || '').toString().toLowerCase();
        var combined = id + ' ' + cls;

        // Check ARIA roles first
        if (role === 'main' || role === 'article') return 'MainContent';
        if (role === 'navigation') return 'Navigation';
        if (role === 'banner') return 'Header';
        if (role === 'contentinfo') return 'Footer';
        if (role === 'complementary') return 'Sidebar';

        // Check semantic tags
        if (tag === 'article' || tag === 'main') return 'MainContent';
        if (tag === 'nav') return 'Navigation';
        if (tag === 'header') return 'Header';
        if (tag === 'footer') return 'Footer';
        if (tag === 'aside') return 'Sidebar';

        // Check class/id patterns
        if (/\b(comment|disqus)\b/.test(combined)) return 'Comments';
        if (/\b(ad|ads|advert|sponsor|banner)\b/.test(combined)) return 'Advertisement';
        if (/\b(related|recommended|popular|trending)\b/.test(combined)) return 'RelatedContent';
        if (/\b(nav|menu|breadcrumb)\b/.test(combined)) return 'Navigation';
        if (/\b(header|masthead|top-bar|topbar)\b/.test(combined)) return 'Header';
        if (/\b(footer|bottom-bar|bottombar)\b/.test(combined)) return 'Footer';
        if (/\b(sidebar|aside|widget)\b/.test(combined)) return 'Sidebar';
        if (/\b(content|article|post|entry|story|main)\b/.test(combined)) return 'MainContent';

        return 'Unknown';
    }

    // --- Section analysis ---

    var sections = [];
    var sectionSelectors = 'article, main, nav, aside, header, footer, section, ' +
        '[role="main"], [role="article"], [role="navigation"], [role="banner"], ' +
        '[role="contentinfo"], [role="complementary"]';
    var sectionElements = document.querySelectorAll(sectionSelectors);

    for (var i = 0; i < sectionElements.length; i++) {
        var el = sectionElements[i];
        var textLen = getTextLength(el);
        if (textLen < 10) continue; // Skip empty sections

        sections.push({
            tag: el.tagName.toLowerCase(),
            role: classifySection(el),
            text_length: textLen,
            link_density: getLinkDensity(el),
            heading: findHeading(el),
            selector: buildSelector(el)
        });
    }

    // If no semantic sections found, analyze top-level divs
    if (sections.length === 0) {
        var topDivs = document.querySelectorAll('body > div, body > section');
        for (var i = 0; i < topDivs.length; i++) {
            var el = topDivs[i];
            var textLen = getTextLength(el);
            if (textLen < 20) continue;

            sections.push({
                tag: el.tagName.toLowerCase(),
                role: classifySection(el),
                text_length: textLen,
                link_density: getLinkDensity(el),
                heading: findHeading(el),
                selector: buildSelector(el)
            });
        }
    }

    // --- Navigation extraction ---

    var navigation = [];
    var navElements = document.querySelectorAll(
        'nav a[href], [role="navigation"] a[href]'
    );
    var seenHrefs = {};
    for (var i = 0; i < navElements.length; i++) {
        var link = navElements[i];
        var href = link.getAttribute('href') || '';
        if (!href || href === '#' || seenHrefs[href]) continue;
        seenHrefs[href] = true;

        var text = link.textContent.trim().substring(0, 100);
        if (!text) continue;

        // Detect active state via class, aria-current, or data attributes
        var isActive = false;
        var linkCls = (link.className || '').toString().toLowerCase();
        if (/\b(active|current|selected)\b/.test(linkCls)) isActive = true;
        if (link.getAttribute('aria-current')) isActive = true;

        navigation.push({
            text: text,
            href: href,
            is_active: isActive
        });
    }

    // --- Page type detection ---

    var pageType = 'Unknown';

    // Check for article signals
    var hasArticle = document.querySelector('article, [itemtype*="Article"]') !== null;
    var hasLongContent = false;
    for (var i = 0; i < sections.length; i++) {
        if (sections[i].role === 'MainContent' && sections[i].text_length > 500) {
            hasLongContent = true;
            break;
        }
    }

    // Check for product page signals
    var hasProduct = document.querySelector(
        '[itemtype*="Product"], [class*="product" i], [class*="price" i], ' +
        '[data-product-id], .add-to-cart, [class*="add-to-cart" i]'
    ) !== null;

    // Check for search results signals
    var hasSearchResults = document.querySelector(
        '[class*="search-result" i], [class*="searchresult" i], ' +
        '[class*="search_result" i], [role="search"] ~ [class*="result" i]'
    ) !== null;
    var searchInput = document.querySelector('input[type="search"], input[name="q"]');

    // Check for listing/category page signals
    var hasListing = document.querySelector(
        '[class*="listing" i], [class*="catalog" i], [class*="category" i], ' +
        '[class*="product-list" i], [class*="item-list" i]'
    ) !== null;

    // Check for login page signals
    var hasLogin = document.querySelector(
        'input[type="password"], [class*="login" i], [class*="signin" i], ' +
        '[class*="sign-in" i], form[action*="login" i], form[action*="signin" i]'
    ) !== null;

    // Check for form page signals
    var formElements = document.querySelectorAll(
        'input:not([type="hidden"]):not([type="search"]), textarea, select'
    );
    var hasForm = formElements.length > 3;

    // Check for landing page signals
    var hasCTA = document.querySelectorAll(
        '[class*="cta" i], [class*="hero" i], [class*="call-to-action" i]'
    ).length > 0;

    // Determine page type by priority
    if (hasLogin) pageType = 'LoginPage';
    else if (hasProduct) pageType = 'ProductPage';
    else if (hasSearchResults && searchInput) pageType = 'SearchResults';
    else if (hasArticle && hasLongContent) pageType = 'Article';
    else if (hasListing) pageType = 'ListingPage';
    else if (hasForm) pageType = 'FormPage';
    else if (hasCTA && !hasLongContent) pageType = 'LandingPage';
    else if (hasLongContent) pageType = 'Article';

    var result = {
        sections: sections,
        navigation: navigation,
        page_type: pageType
    };

    return JSON.stringify(result);
})()
"#
        .to_string()
    }
}

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
