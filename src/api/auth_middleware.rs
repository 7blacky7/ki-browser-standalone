//! API authentication middleware — opt-in Bearer-token gate.
//!
//! This middleware is **disabled by default**: when no token is configured
//! (`State` is `None`), every request passes straight through, preserving the
//! historical "open LAN" behaviour. When a token *is* configured
//! (`KI_BROWSER_API_TOKEN` / `api_token`), protected routes require a matching
//! `Authorization: Bearer <token>` header, otherwise they receive `401`.
//!
//! A small path whitelist (`/health`, `/status`, `/api-doc`, `/swagger-ui`)
//! stays open even with a token set, so health checks and the API docs remain
//! reachable without credentials.

use axum::{
    body::Body,
    extract::State,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

use crate::api::routes::ApiResponse;

/// Returns `true` if the path is always reachable without a token.
///
/// Matches the exact path and any sub-path (e.g. `/swagger-ui/index.html`,
/// `/api-doc/openapi.json`).
fn is_whitelisted(path: &str) -> bool {
    const OPEN_PREFIXES: [&str; 4] = ["/health", "/status", "/api-doc", "/swagger-ui"];
    OPEN_PREFIXES.iter().any(|prefix| {
        path == *prefix || path.starts_with(&format!("{}/", prefix))
    })
}

/// Middleware: enforce Bearer-token auth when a token is configured.
///
/// * `token == None` → pass-through (auth disabled, exactly today's behaviour).
/// * `token == Some(t)`:
///   * whitelisted paths pass through untouched,
///   * otherwise the `Authorization` header must equal `Bearer <t>`,
///   * mismatch / missing header → `401 Unauthorized`.
pub async fn auth_layer(
    State(token): State<Option<Arc<String>>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    // Auth disabled: pass-through (default).
    let Some(expected) = token else {
        return next.run(request).await;
    };

    // Always-open paths (health checks, API docs).
    if is_whitelisted(request.uri().path()) {
        return next.run(request).await;
    }

    // Compare the Authorization header against "Bearer <token>".
    let provided = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    let authorized = matches!(
        provided,
        Some(value) if value.strip_prefix("Bearer ") == Some(expected.as_str())
    );

    if authorized {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::error(
                "Unauthorized: valid Bearer token required",
            )),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};
    use tower::ServiceExt; // for `oneshot`

    /// Build a tiny router with the auth layer wired to `token`.
    fn app(token: Option<Arc<String>>) -> Router {
        Router::new()
            .route("/evaluate", get(|| async { "ok" }))
            .route("/health", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(token, auth_layer))
    }

    fn req(path: &str, auth: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().uri(path);
        if let Some(value) = auth {
            builder = builder.header(header::AUTHORIZATION, value);
        }
        builder.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn no_token_configured_passes_through() {
        let resp = app(None)
            .oneshot(req("/evaluate", None))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn token_set_but_missing_header_is_unauthorized() {
        let token = Some(Arc::new("secret".to_string()));
        let resp = app(token).oneshot(req("/evaluate", None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn token_set_with_correct_bearer_passes() {
        let token = Some(Arc::new("secret".to_string()));
        let resp = app(token)
            .oneshot(req("/evaluate", Some("Bearer secret")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn token_set_with_wrong_bearer_is_unauthorized() {
        let token = Some(Arc::new("secret".to_string()));
        let resp = app(token)
            .oneshot(req("/evaluate", Some("Bearer wrong")))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn health_is_always_open_even_with_token() {
        let token = Some(Arc::new("secret".to_string()));
        let resp = app(token).oneshot(req("/health", None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn whitelist_matches_exact_and_subpaths() {
        assert!(is_whitelisted("/health"));
        assert!(is_whitelisted("/swagger-ui"));
        assert!(is_whitelisted("/swagger-ui/index.html"));
        assert!(is_whitelisted("/api-doc/openapi.json"));
        assert!(!is_whitelisted("/evaluate"));
        assert!(!is_whitelisted("/healthcheck")); // not a sub-path of /health
    }
}
