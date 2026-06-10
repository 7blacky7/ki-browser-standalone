//! Bundle <-> CDP translation: restoring cookies/storage into a fresh tab and
//! exporting them back out of a running tab.
//!
//! Restore order (must run AFTER identity init, BEFORE target navigation):
//!   1. `Network.setCookie` for every cookie (httpOnly/secure-capable).
//!   2. localStorage via `Page.addScriptToEvaluateOnNewDocument` so the very
//!      first real document of the matching origin already has it.
//!   3. sessionStorage cannot be pre-seeded reliably before navigation, so it
//!      is applied after navigation by the caller via [`session_storage_script`].

use std::sync::Arc;

use serde_json::{json, Value};

use crate::api::cdp_client::CdpClient;

use super::types::{Bundle, CookieSpec, StorageEntry};

/// Builds the CDP `Network.setCookie` param object for one cookie.
pub fn cookie_to_cdp_param(c: &CookieSpec) -> Value {
    let mut obj = json!({
        "name": c.name,
        "value": c.value,
        "domain": c.domain,
        "path": c.path,
        "secure": c.secure,
        "httpOnly": c.http_only,
    });
    if let Some(ss) = &c.same_site {
        // CDP expects "Strict" | "Lax" | "None".
        let normalized = match ss.to_ascii_lowercase().as_str() {
            "strict" => Some("Strict"),
            "lax" => Some("Lax"),
            "none" | "no_restriction" => Some("None"),
            _ => None,
        };
        if let Some(v) = normalized {
            obj["sameSite"] = json!(v);
        }
    }
    if let Some(exp) = c.expires {
        obj["expires"] = json!(exp);
    }
    obj
}

/// Restores all cookies of a bundle onto the tab's CDP target.
/// Returns the number of successfully set cookies.
pub async fn restore_cookies(
    cdp: &Arc<CdpClient>,
    ws_url: &str,
    bundle: &Bundle,
) -> Result<usize, String> {
    if bundle.cookies.is_empty() {
        return Ok(0);
    }
    let params: Vec<Value> = bundle.cookies.iter().map(cookie_to_cdp_param).collect();
    cdp.set_cookies(ws_url, &params).await
}

/// JS that seeds localStorage for a specific origin. Safe to run as an
/// init-script (guards on `location.origin`) so it only fires on that origin.
pub fn local_storage_init_script(entry: &StorageEntry) -> String {
    let pairs = serde_json::to_string(&entry.local).unwrap_or_else(|_| "{}".to_string());
    format!(
        r#"(function(){{try{{if(window.location&&window.location.origin==={origin}){{var d={pairs};for(var k in d){{try{{window.localStorage.setItem(k,d[k]);}}catch(e){{}}}}}}}}catch(e){{}}}})();"#,
        origin = serde_json::to_string(&entry.origin).unwrap_or_else(|_| "\"\"".to_string()),
        pairs = pairs
    )
}

/// JS that seeds BOTH localStorage and sessionStorage for the current document.
/// Run via `Runtime.evaluate` AFTER navigating to the entry's origin. This is the
/// reliable restore path in CEF single-process, where
/// `Page.addScriptToEvaluateOnNewDocument` does not consistently fire before the
/// page's own scripts (so the init-script localStorage seeding can be missed).
pub fn post_nav_storage_script(entry: &StorageEntry) -> String {
    let local = serde_json::to_string(&entry.local).unwrap_or_else(|_| "{}".to_string());
    let session = serde_json::to_string(&entry.session).unwrap_or_else(|_| "{}".to_string());
    format!(
        r#"(function(){{try{{var l={local};for(var k in l){{try{{window.localStorage.setItem(k,l[k]);}}catch(e){{}}}}var s={session};for(var k2 in s){{try{{window.sessionStorage.setItem(k2,s[k2]);}}catch(e){{}}}}return true;}}catch(e){{return false;}}}})();"#,
        local = local,
        session = session
    )
}

/// Registers localStorage init-scripts for every storage entry that has local
/// data. Returns how many origins were seeded.
pub async fn restore_local_storage(
    cdp: &Arc<CdpClient>,
    ws_url: &str,
    bundle: &Bundle,
) -> Result<usize, String> {
    let mut count = 0usize;
    for entry in &bundle.storage {
        if entry.local.is_empty() {
            continue;
        }
        let script = local_storage_init_script(entry);
        if cdp.add_init_script(ws_url, &script).await.is_ok() {
            count += 1;
        }
    }
    Ok(count)
}

/// Filters raw CDP cookies (from `Network.getAllCookies`) to those relevant to
/// `origin`'s host (exact or dot-suffix domain match) and converts them to
/// [`CookieSpec`].
pub fn cookies_for_origin(raw: &[Value], origin: &str) -> Vec<CookieSpec> {
    let host = origin
        .split("://")
        .nth(1)
        .unwrap_or(origin)
        .split('/')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    raw.iter()
        .filter(|c| {
            let domain = c
                .get("domain")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .trim_start_matches('.')
                .to_ascii_lowercase();
            !domain.is_empty()
                && (host == domain || host.ends_with(&format!(".{}", domain)))
        })
        .map(cdp_cookie_to_spec)
        .collect()
}

fn cdp_cookie_to_spec(c: &Value) -> CookieSpec {
    CookieSpec {
        name: c.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        value: c.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        domain: c.get("domain").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        path: c.get("path").and_then(|v| v.as_str()).unwrap_or("/").to_string(),
        secure: c.get("secure").and_then(|v| v.as_bool()).unwrap_or(false),
        http_only: c.get("httpOnly").and_then(|v| v.as_bool()).unwrap_or(false),
        same_site: c.get("sameSite").and_then(|v| v.as_str()).map(String::from),
        expires: c.get("expires").and_then(|v| v.as_f64()).filter(|e| *e > 0.0),
    }
}

/// JS that reads the current document's local+sessionStorage as a JSON object
/// `{"local":{...},"session":{...}}`. Run via `Runtime.evaluate` on the origin.
pub fn read_storage_script() -> String {
    r#"(function(){function dump(s){var o={};try{for(var i=0;i<s.length;i++){var k=s.key(i);o[k]=s.getItem(k);}}catch(e){}return o;}return JSON.stringify({local:dump(window.localStorage),session:dump(window.sessionStorage)});})()"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::session_store::types::ScreenSize;

    #[test]
    fn test_cookie_to_cdp_param_maps_httponly_samesite() {
        let c = CookieSpec {
            name: "sid".into(),
            value: "v".into(),
            domain: ".x.test".into(),
            path: "/".into(),
            secure: true,
            http_only: true,
            same_site: Some("none".into()),
            expires: Some(123.0),
        };
        let p = cookie_to_cdp_param(&c);
        assert_eq!(p["httpOnly"], json!(true));
        assert_eq!(p["secure"], json!(true));
        assert_eq!(p["sameSite"], json!("None"));
        assert_eq!(p["expires"], json!(123.0));
    }

    #[test]
    fn test_cookies_for_origin_matches_dot_domain() {
        let raw = vec![
            json!({"name":"a","value":"1","domain":".x.test","path":"/","secure":true,"httpOnly":false}),
            json!({"name":"b","value":"2","domain":"other.test","path":"/"}),
            json!({"name":"c","value":"3","domain":"www.x.test","path":"/"}),
        ];
        let got = cookies_for_origin(&raw, "https://www.x.test/login");
        let names: Vec<&str> = got.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"c"));
        assert!(!names.contains(&"b"));
    }

    #[test]
    fn test_local_storage_init_script_guards_origin() {
        let entry = StorageEntry {
            origin: "https://x.test".into(),
            local: [("k".to_string(), "v".to_string())].into_iter().collect(),
            session: Default::default(),
        };
        let s = local_storage_init_script(&entry);
        assert!(s.contains("location.origin"));
        assert!(s.contains("https://x.test"));
        assert!(s.contains("localStorage.setItem"));
    }

    #[test]
    fn test_post_nav_storage_script_contains_both() {
        let entry = StorageEntry {
            origin: "https://x.test".into(),
            local: [("lk".to_string(), "lv".to_string())].into_iter().collect(),
            session: [("s".to_string(), "1".to_string())].into_iter().collect(),
        };
        let s = post_nav_storage_script(&entry);
        assert!(s.contains("localStorage.setItem"));
        assert!(s.contains("sessionStorage.setItem"));
        assert!(s.contains("\"lk\""));
        assert!(s.contains("\"s\""));
    }

    #[test]
    fn test_screensize_is_used() {
        // Keep ScreenSize import meaningful for this test module.
        let s = ScreenSize { width: 1, height: 2 };
        assert_eq!(s.width + s.height, 3);
    }
}
