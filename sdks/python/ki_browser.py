"""
ki-browser Python SDK

Handwritten SDK for the ki-browser REST API.
Provides a clean, typed interface for browser automation via HTTP.

Requirements:
    pip install requests

Usage:
    from ki_browser import KiBrowser

    browser = KiBrowser("http://localhost:9222")
    browser.navigate("https://example.com")
    tabs = browser.list_tabs()
    screenshot = browser.screenshot()
"""

from __future__ import annotations

import base64
from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional

import requests


# ============================================================================
# Response Types
# ============================================================================


@dataclass
class HealthStatus:
    """Health check response from the API server."""

    status: str
    version: str
    api_enabled: bool


@dataclass
class TabInfo:
    """Information about a browser tab."""

    id: str
    url: str
    title: str
    is_loading: bool
    is_active: bool
    can_go_back: bool
    can_go_forward: bool


@dataclass
class TabsResponse:
    """Response from listing all tabs."""

    tabs: List[TabInfo]
    active_tab_id: Optional[str]


@dataclass
class NewTabResponse:
    """Response from creating a new tab."""

    tab_id: str
    url: str


@dataclass
class BoundingBox:
    """Bounding box of a DOM element in pixels."""

    x: float
    y: float
    width: float
    height: float


@dataclass
class ElementInfo:
    """Information about a found DOM element."""

    found: bool
    tag_name: Optional[str] = None
    text_content: Optional[str] = None
    attributes: Optional[Dict[str, Any]] = None
    bounding_box: Optional[BoundingBox] = None
    is_visible: Optional[bool] = None


@dataclass
class ScreenshotResponse:
    """Screenshot capture response with base64 encoded image data."""

    data: str
    format: str
    width: int
    height: int

    def save(self, path: str) -> None:
        """Save the screenshot to a file."""
        raw = base64.b64decode(self.data)
        with open(path, "wb") as f:
            f.write(raw)


@dataclass
class EvaluateResponse:
    """Response from JavaScript evaluation."""

    result: Any


@dataclass
class ApiStatus:
    """API server status."""

    enabled: bool
    port: int
    connected_clients: int


# ============================================================================
# Exceptions
# ============================================================================


class KiBrowserError(Exception):
    """Base exception for ki-browser SDK errors."""

    def __init__(self, message: str, status_code: Optional[int] = None):
        super().__init__(message)
        self.status_code = status_code


class ApiDisabledError(KiBrowserError):
    """Raised when the API is disabled (HTTP 503)."""

    pass


class BadRequestError(KiBrowserError):
    """Raised for invalid requests (HTTP 400)."""

    pass


# ============================================================================
# SDK Client
# ============================================================================


class KiBrowser:
    """Python SDK client for the ki-browser REST API.

    Provides methods for all browser automation endpoints including
    tab management, navigation, DOM interaction, and screenshots.

    Args:
        base_url: Base URL of the ki-browser API server (e.g. "http://localhost:9222").
        timeout: Request timeout in seconds. Defaults to 30.

    Example:
        >>> browser = KiBrowser("http://localhost:9222")
        >>> browser.navigate("https://example.com")
        >>> tabs = browser.list_tabs()
        >>> print(tabs.tabs[0].title)
    """

    def __init__(self, base_url: str = "http://localhost:9222", timeout: int = 30):
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self.session = requests.Session()
        self.session.headers.update({"Content-Type": "application/json"})

    def _url(self, path: str) -> str:
        """Build a full URL from a path."""
        return f"{self.base_url}{path}"

    def _handle_response(self, resp: requests.Response) -> Dict[str, Any]:
        """Parse API response and raise on errors."""
        if resp.status_code == 503:
            raise ApiDisabledError("API is disabled", status_code=503)

        data = resp.json()

        if not data.get("success", False):
            error_msg = data.get("error", "Unknown error")
            if resp.status_code == 400:
                raise BadRequestError(error_msg, status_code=400)
            raise KiBrowserError(error_msg, status_code=resp.status_code)

        return data.get("data") or {}

    def _get(self, path: str, params: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        """Send a GET request."""
        resp = self.session.get(self._url(path), params=params, timeout=self.timeout)
        return self._handle_response(resp)

    def _post(self, path: str, json: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        """Send a POST request."""
        resp = self.session.post(self._url(path), json=json or {}, timeout=self.timeout)
        return self._handle_response(resp)

    # ========================================================================
    # Health
    # ========================================================================

    def health(self) -> HealthStatus:
        """Check server health.

        Returns:
            HealthStatus with server status, version, and API enabled flag.
        """
        data = self._get("/health")
        return HealthStatus(
            status=data["status"],
            version=data["version"],
            api_enabled=data["api_enabled"],
        )

    # ========================================================================
    # Tab Management
    # ========================================================================

    def list_tabs(self) -> TabsResponse:
        """List all open browser tabs.

        Returns:
            TabsResponse with list of tabs and active tab ID.
        """
        data = self._get("/tabs")
        tabs = [
            TabInfo(
                id=t["id"],
                url=t["url"],
                title=t["title"],
                is_loading=t["is_loading"],
                is_active=t["is_active"],
                can_go_back=t["can_go_back"],
                can_go_forward=t["can_go_forward"],
            )
            for t in data.get("tabs", [])
        ]
        return TabsResponse(tabs=tabs, active_tab_id=data.get("active_tab_id"))

    def new_tab(self, url: Optional[str] = None, active: bool = True) -> NewTabResponse:
        """Create a new browser tab.

        Args:
            url: URL to open in the new tab. Defaults to about:blank.
            active: Whether to make the new tab active. Defaults to True.

        Returns:
            NewTabResponse with the new tab ID and URL.
        """
        body: Dict[str, Any] = {"active": active}
        if url is not None:
            body["url"] = url
        data = self._post("/tabs/new", json=body)
        return NewTabResponse(tab_id=data["tab_id"], url=data["url"])

    def close_tab(self, tab_id: str) -> None:
        """Close a browser tab.

        Args:
            tab_id: ID of the tab to close.
        """
        self._post("/tabs/close", json={"tab_id": tab_id})

    # ========================================================================
    # Navigation & Interaction
    # ========================================================================

    def navigate(self, url: str, tab_id: Optional[str] = None) -> None:
        """Navigate to a URL.

        Args:
            url: The URL to navigate to.
            tab_id: Target tab ID. Uses active tab if omitted.
        """
        body: Dict[str, Any] = {"url": url}
        if tab_id is not None:
            body["tab_id"] = tab_id
        self._post("/navigate", json=body)

    def click(
        self,
        x: Optional[int] = None,
        y: Optional[int] = None,
        selector: Optional[str] = None,
        button: str = "left",
        tab_id: Optional[str] = None,
    ) -> None:
        """Click at coordinates or on a CSS selector.

        Provide either (x, y) coordinates or a CSS selector, not both.

        Args:
            x: X coordinate to click.
            y: Y coordinate to click.
            selector: CSS selector of element to click.
            button: Mouse button ("left", "right", "middle"). Defaults to "left".
            tab_id: Target tab ID. Uses active tab if omitted.
        """
        body: Dict[str, Any] = {"button": button}
        if x is not None:
            body["x"] = x
        if y is not None:
            body["y"] = y
        if selector is not None:
            body["selector"] = selector
        if tab_id is not None:
            body["tab_id"] = tab_id
        self._post("/click", json=body)

    def type_text(
        self,
        text: str,
        selector: Optional[str] = None,
        clear_first: bool = False,
        tab_id: Optional[str] = None,
    ) -> None:
        """Type text into the focused element or a specified selector.

        Args:
            text: Text to type.
            selector: CSS selector of element to type into.
            clear_first: Whether to clear the field before typing.
            tab_id: Target tab ID. Uses active tab if omitted.
        """
        body: Dict[str, Any] = {"text": text, "clear_first": clear_first}
        if selector is not None:
            body["selector"] = selector
        if tab_id is not None:
            body["tab_id"] = tab_id
        self._post("/type", json=body)

    def scroll(
        self,
        x: Optional[int] = None,
        y: Optional[int] = None,
        delta_x: Optional[int] = None,
        delta_y: Optional[int] = None,
        selector: Optional[str] = None,
        behavior: Optional[str] = None,
        tab_id: Optional[str] = None,
    ) -> None:
        """Scroll the page.

        Args:
            x: Absolute scroll X position.
            y: Absolute scroll Y position.
            delta_x: Relative horizontal scroll amount.
            delta_y: Relative vertical scroll amount.
            selector: CSS selector of element to scroll into view.
            behavior: Scroll behavior ("auto", "smooth", "instant").
            tab_id: Target tab ID. Uses active tab if omitted.
        """
        body: Dict[str, Any] = {}
        if x is not None:
            body["x"] = x
        if y is not None:
            body["y"] = y
        if delta_x is not None:
            body["delta_x"] = delta_x
        if delta_y is not None:
            body["delta_y"] = delta_y
        if selector is not None:
            body["selector"] = selector
        if behavior is not None:
            body["behavior"] = behavior
        if tab_id is not None:
            body["tab_id"] = tab_id
        self._post("/scroll", json=body)

    def evaluate(
        self,
        script: str,
        await_promise: bool = True,
        tab_id: Optional[str] = None,
    ) -> Any:
        """Execute JavaScript in the browser context.

        Args:
            script: JavaScript code to evaluate.
            await_promise: Whether to await the result if it is a Promise.
            tab_id: Target tab ID. Uses active tab if omitted.

        Returns:
            The evaluation result (parsed from JSON).
        """
        body: Dict[str, Any] = {"script": script, "await_promise": await_promise}
        if tab_id is not None:
            body["tab_id"] = tab_id
        data = self._post("/evaluate", json=body)
        return data.get("result")

    # ========================================================================
    # Screenshots
    # ========================================================================

    def screenshot(
        self,
        format: str = "png",
        quality: Optional[int] = None,
        full_page: bool = False,
        selector: Optional[str] = None,
        tab_id: Optional[str] = None,
    ) -> ScreenshotResponse:
        """Capture a screenshot of the current page.

        Args:
            format: Image format ("png" or "jpeg"). Defaults to "png".
            quality: JPEG quality (0-100). Only used for JPEG format.
            full_page: Whether to capture the full scrollable page.
            selector: CSS selector of element to capture.
            tab_id: Target tab ID. Uses active tab if omitted.

        Returns:
            ScreenshotResponse with base64-encoded image data.
        """
        params: Dict[str, Any] = {"format": format}
        if quality is not None:
            params["quality"] = quality
        if full_page:
            params["full_page"] = "true"
        if selector is not None:
            params["selector"] = selector
        if tab_id is not None:
            params["tab_id"] = tab_id
        data = self._get("/screenshot", params=params)
        return ScreenshotResponse(
            data=data["data"],
            format=data["format"],
            width=data["width"],
            height=data["height"],
        )

    # ========================================================================
    # DOM Operations
    # ========================================================================

    def find_element(
        self,
        selector: str,
        timeout: Optional[int] = None,
        tab_id: Optional[str] = None,
    ) -> ElementInfo:
        """Find a DOM element by CSS selector.

        Args:
            selector: CSS selector to search for.
            timeout: Timeout in milliseconds to wait for the element.
            tab_id: Target tab ID. Uses active tab if omitted.

        Returns:
            ElementInfo with element details if found.
        """
        params: Dict[str, Any] = {"selector": selector}
        if timeout is not None:
            params["timeout"] = timeout
        if tab_id is not None:
            params["tab_id"] = tab_id
        data = self._get("/dom/element", params=params)
        bbox = None
        if data.get("bounding_box"):
            b = data["bounding_box"]
            bbox = BoundingBox(x=b["x"], y=b["y"], width=b["width"], height=b["height"])
        return ElementInfo(
            found=data["found"],
            tag_name=data.get("tag_name"),
            text_content=data.get("text_content"),
            attributes=data.get("attributes"),
            bounding_box=bbox,
            is_visible=data.get("is_visible"),
        )

    # ========================================================================
    # API Management
    # ========================================================================

    def api_status(self) -> ApiStatus:
        """Get current API server status.

        Returns:
            ApiStatus with enabled flag, port, and connected client count.
        """
        data = self._get("/api/status")
        return ApiStatus(
            enabled=data["enabled"],
            port=data["port"],
            connected_clients=data["connected_clients"],
        )

    def toggle_api(self, enabled: bool) -> ApiStatus:
        """Toggle API enabled state.

        Args:
            enabled: Whether to enable or disable the API.

        Returns:
            Updated ApiStatus.
        """
        data = self._post("/api/toggle", json={"enabled": enabled})
        return ApiStatus(
            enabled=data["enabled"],
            port=data["port"],
            connected_clients=data["connected_clients"],
        )


# ============================================================================
# Example Usage
# ============================================================================

if __name__ == "__main__":
    browser = KiBrowser("http://localhost:9222")

    # Check health
    health = browser.health()
    print(f"Server: {health.version} - {health.status}")

    # Create a tab and navigate
    tab = browser.new_tab("https://example.com")
    print(f"Created tab: {tab.tab_id}")

    # List tabs
    tabs = browser.list_tabs()
    for t in tabs.tabs:
        print(f"  Tab {t.id}: {t.title} ({t.url})")

    # Take a screenshot
    shot = browser.screenshot(format="png")
    shot.save("screenshot.png")
    print(f"Screenshot saved: {shot.width}x{shot.height}")

    # Find an element
    el = browser.find_element("h1")
    if el.found:
        print(f"Found <{el.tag_name}>: {el.text_content}")

    # Run JavaScript
    result = browser.evaluate("document.title")
    print(f"Page title: {result}")

    # Close the tab
    browser.close_tab(tab.tab_id)
