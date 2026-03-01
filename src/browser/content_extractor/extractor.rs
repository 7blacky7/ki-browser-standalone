//! ContentExtractor implementation with browser-side JavaScript generation.
//!
//! Produces two independent scripts for browser-context execution:
//! - [`content_extraction_script`](ContentExtractor::content_extraction_script): Readability-like
//!   main content extraction using text-density and link-density scoring
//! - [`structure_analysis_script`](ContentExtractor::structure_analysis_script): Full page
//!   structure analysis with semantic tag recognition and ARIA role classification

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
