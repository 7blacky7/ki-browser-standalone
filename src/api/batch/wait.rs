//! Wait condition timeout resolution and JavaScript expression generation.
//!
//! Implements `WaitCondition` methods that convert high-level wait
//! specifications (CSS selector presence, navigation completion,
//! network idle, fixed delay, custom JS function) into executable
//! JavaScript polling scripts for the browser context.

use super::types::WaitCondition;

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
            WaitCondition::Selector {
                selector,
                timeout_ms,
            } => {
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
                format!("new Promise(resolve => setTimeout(resolve, {ms}))")
            }
            WaitCondition::Function {
                expression,
                timeout_ms,
            } => {
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
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::super::types::WaitCondition;

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
}
