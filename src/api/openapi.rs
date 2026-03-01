//! OpenAPI specification and Swagger UI integration for ki-browser REST API.
//!
//! Provides auto-generated OpenAPI 3.1 documentation via utoipa and serves
//! an interactive Swagger UI at `/swagger-ui/`. The JSON spec is available
//! at `/api-doc/openapi.json`.

use utoipa::OpenApi;

use crate::api::routes::{
    ApiStatusResponse, ApiToggleRequest, BoundingBox, ClickRequest, CloseTabRequest, ElementInfo,
    EvaluateRequest, EvaluateResponse, FindElementQuery, HealthResponse, NavigateRequest,
    NewTabRequest, NewTabResponse, ScreenshotQuery, ScreenshotResponse, ScrollRequest, TabInfo,
    TabsResponse, TypeRequest,
};

/// OpenAPI documentation for the ki-browser REST API.
///
/// Aggregates all endpoint paths and schema types into a single OpenAPI spec
/// that can be served as JSON and rendered by Swagger UI.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "ki-browser API",
        version = "0.1.0",
        description = "High-performance browser automation REST API with stealth capabilities",
        license(name = "MIT")
    ),
    paths(
        crate::api::routes::health_check,
        crate::api::routes::list_tabs,
        crate::api::routes::create_tab,
        crate::api::routes::close_tab,
        crate::api::routes::navigate,
        crate::api::routes::click,
        crate::api::routes::type_text,
        crate::api::routes::evaluate,
        crate::api::routes::screenshot,
        crate::api::routes::scroll,
        crate::api::routes::find_element,
        crate::api::routes::toggle_api,
        crate::api::routes::api_status,
    ),
    components(schemas(
        HealthResponse,
        TabInfo,
        TabsResponse,
        NewTabRequest,
        NewTabResponse,
        CloseTabRequest,
        NavigateRequest,
        ClickRequest,
        TypeRequest,
        EvaluateRequest,
        EvaluateResponse,
        ScreenshotQuery,
        ScreenshotResponse,
        ScrollRequest,
        FindElementQuery,
        ElementInfo,
        BoundingBox,
        ApiToggleRequest,
        ApiStatusResponse,
    )),
    tags(
        (name = "health", description = "Health check endpoint"),
        (name = "tabs", description = "Browser tab management"),
        (name = "navigation", description = "Page navigation and interaction"),
        (name = "dom", description = "DOM element operations"),
        (name = "api", description = "API management endpoints"),
    )
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::OpenApi;

    #[test]
    fn test_openapi_spec_generates_valid_json() {
        let spec = ApiDoc::openapi();
        let json = spec.to_json();
        assert!(json.is_ok(), "OpenAPI spec should serialize to valid JSON");
        let json_str = json.unwrap();
        assert!(json_str.contains("ki-browser API"));
        assert!(json_str.contains("/health"));
        assert!(json_str.contains("/tabs"));
    }

    #[test]
    fn test_openapi_spec_contains_all_paths() {
        let spec = ApiDoc::openapi();
        let json_str = spec.to_json().unwrap();
        let expected_paths = [
            "/health",
            "/tabs",
            "/tabs/new",
            "/tabs/close",
            "/navigate",
            "/click",
            "/type",
            "/evaluate",
            "/screenshot",
            "/scroll",
            "/dom/element",
            "/api/toggle",
            "/api/status",
        ];
        for path in expected_paths {
            assert!(
                json_str.contains(path),
                "OpenAPI spec should contain path: {}",
                path
            );
        }
    }

    #[test]
    fn test_openapi_spec_contains_schemas() {
        let spec = ApiDoc::openapi();
        let json_str = spec.to_json().unwrap();
        let expected_schemas = [
            "HealthResponse",
            "TabInfo",
            "NewTabRequest",
            "ClickRequest",
            "EvaluateRequest",
            "ScrollRequest",
        ];
        for schema in expected_schemas {
            assert!(
                json_str.contains(schema),
                "OpenAPI spec should contain schema: {}",
                schema
            );
        }
    }
}
