/**
 * ki-browser TypeScript SDK
 *
 * Handwritten SDK for the ki-browser REST API.
 * Uses native fetch — works in Node.js 18+, Deno, Bun, and modern browsers.
 *
 * @example
 * ```typescript
 * import { KiBrowser } from "./ki-browser";
 *
 * const browser = new KiBrowser("http://localhost:9222");
 * await browser.navigate("https://example.com");
 * const tabs = await browser.listTabs();
 * const screenshot = await browser.screenshot();
 * ```
 */

// ============================================================================
// Response Types
// ============================================================================

/** Health check response from the API server. */
export interface HealthStatus {
  status: string;
  version: string;
  api_enabled: boolean;
}

/** Information about a browser tab. */
export interface TabInfo {
  id: string;
  url: string;
  title: string;
  is_loading: boolean;
  is_active: boolean;
  can_go_back: boolean;
  can_go_forward: boolean;
}

/** Response from listing all tabs. */
export interface TabsResponse {
  tabs: TabInfo[];
  active_tab_id: string | null;
}

/** Response from creating a new tab. */
export interface NewTabResponse {
  tab_id: string;
  url: string;
}

/** Bounding box of a DOM element in pixels. */
export interface BoundingBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Information about a found DOM element. */
export interface ElementInfo {
  found: boolean;
  tag_name?: string;
  text_content?: string;
  attributes?: Record<string, unknown>;
  bounding_box?: BoundingBox;
  is_visible?: boolean;
}

/** Screenshot capture response with base64 encoded image data. */
export interface ScreenshotResponse {
  data: string;
  format: string;
  width: number;
  height: number;
}

/** Response from JavaScript evaluation. */
export interface EvaluateResponse {
  result: unknown;
}

/** API server status. */
export interface ApiStatus {
  enabled: boolean;
  port: number;
  connected_clients: number;
}

// ============================================================================
// Request Types
// ============================================================================

/** Options for creating a new tab. */
export interface NewTabOptions {
  url?: string;
  active?: boolean;
}

/** Options for clicking. */
export interface ClickOptions {
  x?: number;
  y?: number;
  selector?: string;
  button?: "left" | "right" | "middle";
  tab_id?: string;
}

/** Options for typing text. */
export interface TypeOptions {
  selector?: string;
  clear_first?: boolean;
  tab_id?: string;
}

/** Options for scrolling. */
export interface ScrollOptions {
  x?: number;
  y?: number;
  delta_x?: number;
  delta_y?: number;
  selector?: string;
  behavior?: "auto" | "smooth" | "instant";
  tab_id?: string;
}

/** Options for taking a screenshot. */
export interface ScreenshotOptions {
  format?: "png" | "jpeg";
  quality?: number;
  full_page?: boolean;
  selector?: string;
  tab_id?: string;
}

/** Options for finding a DOM element. */
export interface FindElementOptions {
  timeout?: number;
  tab_id?: string;
}

/** Options for evaluating JavaScript. */
export interface EvaluateOptions {
  await_promise?: boolean;
  tab_id?: string;
}

// ============================================================================
// Errors
// ============================================================================

/** Base error for ki-browser SDK operations. */
export class KiBrowserError extends Error {
  public statusCode?: number;

  constructor(message: string, statusCode?: number) {
    super(message);
    this.name = "KiBrowserError";
    this.statusCode = statusCode;
  }
}

/** Raised when the API is disabled (HTTP 503). */
export class ApiDisabledError extends KiBrowserError {
  constructor(message: string = "API is disabled") {
    super(message, 503);
    this.name = "ApiDisabledError";
  }
}

/** Raised for invalid requests (HTTP 400). */
export class BadRequestError extends KiBrowserError {
  constructor(message: string) {
    super(message, 400);
    this.name = "BadRequestError";
  }
}

// ============================================================================
// API Response Wrapper
// ============================================================================

interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

// ============================================================================
// SDK Client
// ============================================================================

/**
 * TypeScript SDK client for the ki-browser REST API.
 *
 * Provides async methods for all browser automation endpoints including
 * tab management, navigation, DOM interaction, and screenshots.
 *
 * @example
 * ```typescript
 * const browser = new KiBrowser("http://localhost:9222");
 *
 * // Check health
 * const health = await browser.health();
 * console.log(`Server: ${health.version}`);
 *
 * // Navigate and take a screenshot
 * await browser.navigate("https://example.com");
 * const shot = await browser.screenshot({ format: "png" });
 * console.log(`Screenshot: ${shot.width}x${shot.height}`);
 * ```
 */
export class KiBrowser {
  private baseUrl: string;
  private timeout: number;

  /**
   * Create a new KiBrowser client.
   *
   * @param baseUrl - Base URL of the ki-browser API server.
   * @param timeout - Request timeout in milliseconds. Defaults to 30000.
   */
  constructor(baseUrl: string = "http://localhost:9222", timeout: number = 30000) {
    this.baseUrl = baseUrl.replace(/\/+$/, "");
    this.timeout = timeout;
  }

  // ========================================================================
  // Internal helpers
  // ========================================================================

  private url(path: string): string {
    return `${this.baseUrl}${path}`;
  }

  private async handleResponse<T>(resp: Response): Promise<T> {
    if (resp.status === 503) {
      throw new ApiDisabledError();
    }

    const body: ApiResponse<T> = await resp.json();

    if (!body.success) {
      const msg = body.error ?? "Unknown error";
      if (resp.status === 400) {
        throw new BadRequestError(msg);
      }
      throw new KiBrowserError(msg, resp.status);
    }

    return body.data as T;
  }

  private async get<T>(path: string, params?: Record<string, string>): Promise<T> {
    const queryString = params
      ? "?" + new URLSearchParams(params).toString()
      : "";

    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeout);

    try {
      const resp = await fetch(this.url(path) + queryString, {
        method: "GET",
        headers: { "Content-Type": "application/json" },
        signal: controller.signal,
      });
      return this.handleResponse<T>(resp);
    } finally {
      clearTimeout(timer);
    }
  }

  private async post<T>(path: string, body?: Record<string, unknown>): Promise<T> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeout);

    try {
      const resp = await fetch(this.url(path), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body ?? {}),
        signal: controller.signal,
      });
      return this.handleResponse<T>(resp);
    } finally {
      clearTimeout(timer);
    }
  }

  // ========================================================================
  // Health
  // ========================================================================

  /** Check server health. */
  async health(): Promise<HealthStatus> {
    return this.get<HealthStatus>("/health");
  }

  // ========================================================================
  // Tab Management
  // ========================================================================

  /** List all open browser tabs. */
  async listTabs(): Promise<TabsResponse> {
    return this.get<TabsResponse>("/tabs");
  }

  /**
   * Create a new browser tab.
   *
   * @param options - Optional URL and active state.
   */
  async newTab(options?: NewTabOptions): Promise<NewTabResponse> {
    return this.post<NewTabResponse>("/tabs/new", options);
  }

  /**
   * Close a browser tab.
   *
   * @param tabId - ID of the tab to close.
   */
  async closeTab(tabId: string): Promise<void> {
    await this.post<void>("/tabs/close", { tab_id: tabId });
  }

  // ========================================================================
  // Navigation & Interaction
  // ========================================================================

  /**
   * Navigate to a URL.
   *
   * @param url - The URL to navigate to.
   * @param tabId - Target tab ID. Uses active tab if omitted.
   */
  async navigate(url: string, tabId?: string): Promise<void> {
    const body: Record<string, unknown> = { url };
    if (tabId !== undefined) body.tab_id = tabId;
    await this.post<void>("/navigate", body);
  }

  /**
   * Click at coordinates or on a CSS selector.
   *
   * Provide either (x, y) coordinates or a CSS selector.
   */
  async click(options: ClickOptions): Promise<void> {
    await this.post<void>("/click", options as Record<string, unknown>);
  }

  /**
   * Type text into the focused element or a specified selector.
   *
   * @param text - Text to type.
   * @param options - Optional selector, clear_first, and tab_id.
   */
  async type(text: string, options?: TypeOptions): Promise<void> {
    await this.post<void>("/type", { text, ...options });
  }

  /**
   * Scroll the page.
   *
   * @param options - Scroll position, delta, selector, behavior, tab_id.
   */
  async scroll(options: ScrollOptions): Promise<void> {
    await this.post<void>("/scroll", options as Record<string, unknown>);
  }

  /**
   * Execute JavaScript in the browser context.
   *
   * @param script - JavaScript code to evaluate.
   * @param options - Optional await_promise and tab_id.
   * @returns The evaluation result.
   */
  async evaluate(script: string, options?: EvaluateOptions): Promise<unknown> {
    const data = await this.post<EvaluateResponse>("/evaluate", {
      script,
      ...options,
    });
    return data.result;
  }

  // ========================================================================
  // Screenshots
  // ========================================================================

  /**
   * Capture a screenshot of the current page.
   *
   * @param options - Format, quality, full_page, selector, tab_id.
   */
  async screenshot(options?: ScreenshotOptions): Promise<ScreenshotResponse> {
    const params: Record<string, string> = {};
    if (options?.format) params.format = options.format;
    if (options?.quality !== undefined) params.quality = String(options.quality);
    if (options?.full_page) params.full_page = "true";
    if (options?.selector) params.selector = options.selector;
    if (options?.tab_id) params.tab_id = options.tab_id;

    return this.get<ScreenshotResponse>("/screenshot", params);
  }

  // ========================================================================
  // DOM Operations
  // ========================================================================

  /**
   * Find a DOM element by CSS selector.
   *
   * @param selector - CSS selector to search for.
   * @param options - Optional timeout and tab_id.
   */
  async findElement(selector: string, options?: FindElementOptions): Promise<ElementInfo> {
    const params: Record<string, string> = { selector };
    if (options?.timeout !== undefined) params.timeout = String(options.timeout);
    if (options?.tab_id) params.tab_id = options.tab_id;

    return this.get<ElementInfo>("/dom/element", params);
  }

  // ========================================================================
  // API Management
  // ========================================================================

  /** Get current API server status. */
  async apiStatus(): Promise<ApiStatus> {
    return this.get<ApiStatus>("/api/status");
  }

  /**
   * Toggle API enabled state.
   *
   * @param enabled - Whether to enable or disable the API.
   */
  async toggleApi(enabled: boolean): Promise<ApiStatus> {
    return this.post<ApiStatus>("/api/toggle", { enabled });
  }
}

// ============================================================================
// Example Usage
// ============================================================================

async function main() {
  const browser = new KiBrowser("http://localhost:9222");

  // Check health
  const health = await browser.health();
  console.log(`Server: ${health.version} - ${health.status}`);

  // Create a tab and navigate
  const tab = await browser.newTab({ url: "https://example.com" });
  console.log(`Created tab: ${tab.tab_id}`);

  // List tabs
  const tabs = await browser.listTabs();
  for (const t of tabs.tabs) {
    console.log(`  Tab ${t.id}: ${t.title} (${t.url})`);
  }

  // Take a screenshot
  const shot = await browser.screenshot({ format: "png" });
  console.log(`Screenshot: ${shot.width}x${shot.height}`);

  // Find an element
  const el = await browser.findElement("h1");
  if (el.found) {
    console.log(`Found <${el.tag_name}>: ${el.text_content}`);
  }

  // Run JavaScript
  const result = await browser.evaluate("document.title");
  console.log(`Page title: ${result}`);

  // Close the tab
  await browser.closeTab(tab.tab_id);
}

// Run example if executed directly
if (typeof require !== "undefined" && require.main === module) {
  main().catch(console.error);
}
