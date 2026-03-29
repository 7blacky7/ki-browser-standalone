//! CEF message loop, initialization, and browser instance creation.
//!
//! Contains the synchronous message loop that runs on a dedicated thread,
//! processes CEF work and dispatches commands from the async API. Also
//! provides internal functions for creating and closing browser instances
//! on the CEF thread where single-threaded access is required.

use anyhow::{anyhow, Result};
use cef::{
    BrowserSettings, CefString, MainArgs, Rect, Settings, WindowInfo, LogSeverity,
    ImplBrowser, ImplBrowserHost,
    sys,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::browser::engine::BrowserConfig;
use crate::stealth::StealthConfig;
use super::callbacks::{
    KiBrowserApp, KiBrowserClient, KiBrowserLifeSpanHandlerImpl, KiBrowserLoadHandlerImpl,
    KiBrowserRenderHandlerImpl, KiBrowserDisplayHandlerImpl, KiBrowserRenderProcessHandler,
};
use super::tab::CefTab;
use super::{CefCommand, CEF_MESSAGE_LOOP_DELAY_MS, DEFAULT_FRAME_RATE};

/// Runs the CEF message loop on a dedicated thread.
///
/// Initializes the CEF framework with the provided configuration, then enters
/// a loop that alternates between processing CEF work and handling commands
/// from the async API. This function blocks until a Shutdown command is
/// received or the command channel is disconnected.
pub(crate) fn run_cef_message_loop(
    config: BrowserConfig,
    stealth_config: Arc<StealthConfig>,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    is_running: Arc<AtomicBool>,
    browser_id_counter: Arc<AtomicI32>,
    cef_initialized: Arc<AtomicBool>,
    mut command_rx: mpsc::UnboundedReceiver<CefCommand>,
) -> Result<()> {
    // Find CEF directory (build output or ./cef/)
    let cef_dir = super::engine::CefBrowserEngine::find_cef_dir_static();
    info!("CEF directory: {:?}", cef_dir);

    // Configure CEF settings - use run_message_loop() style (not external pump)
    let mut settings = Settings {
        windowless_rendering_enabled: 1,
        no_sandbox: 1,
        multi_threaded_message_loop: 0,
        external_message_pump: 1, // We pump CEF via do_message_loop_work()
        ..Default::default()
    };

    // Set unique cache path to avoid singleton conflicts
    let cache_dir = format!("/tmp/ki-browser-cef-{}", std::process::id());
    settings.root_cache_path = CefString::from(cache_dir.as_str());
    settings.cache_path = CefString::from(cache_dir.as_str());

    if config.headless {
        settings.windowless_rendering_enabled = 1;
    }

    // Set user agent if provided
    if let Some(ref user_agent) = config.user_agent {
        settings.user_agent = CefString::from(user_agent.as_str());
    }

    // Enable CDP remote debugging if configured (used by Playwright/DevTools)
    if let Some(port) = config.cdp_port {
        if port > 0 {
            settings.remote_debugging_port = port as i32;
            info!("CDP remote debugging enabled on port {}", port);
        }
    }

    // Set CEF resource paths if CEF directory is found (needed when binary != CEF dir)
    if let Some(ref dir) = cef_dir {
        let dir_str = dir.to_string_lossy();
        settings.resources_dir_path = CefString::from(dir_str.as_ref());
        let locales = dir.join("locales");
        if locales.exists() {
            settings.locales_dir_path = CefString::from(locales.to_string_lossy().as_ref());
        }
    }

    // Set log level
    settings.log_severity = LogSeverity::WARNING;

    // CRITICAL: Initialize CEF API version BEFORE anything else
    // Without this, CEF v144 rejects all handler structs with "invalid version -1"
    let _ = cef::api_hash(sys::CEF_API_VERSION_LAST, 0);

    // Call execute_process for subprocess support (returns -1 for browser process)
    let args = MainArgs::default();
    let ret = cef::execute_process(Some(&args), None, std::ptr::null_mut());
    if ret >= 0 {
        // This is a subprocess, exit with the return code
        std::process::exit(ret);
    }
    // ret == -1 means we are the browser process, continue

    // Create render process handler for MessageRouter context hooks
    let rph = KiBrowserRenderProcessHandler::new();

    // Create app with v144 API (wrap_app! macro generates ::new())
    let mut app = KiBrowserApp::new(stealth_config.clone(), rph, config.headless);

    // Initialize CEF using v144 API
    let result = cef::initialize(
        Some(&args),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    );

    if result == 0 {
        return Err(anyhow!("Failed to initialize CEF"));
    }

    info!("CEF initialized successfully");

    cef_initialized.store(true, Ordering::SeqCst);
    is_running.store(true, Ordering::SeqCst);

    // Message loop
    'main_loop: loop {
        // Process CEF work
        cef::do_message_loop_work();

        // Drain ALL pending commands (not just one per iteration)
        loop {
            match command_rx.try_recv() {
                Ok(command) => {
                    match command {
                        CefCommand::CreateBrowser {
                            url,
                            tab_id,
                            response,
                        } => {
                            let result = create_browser_internal(
                                &url,
                                tab_id,
                                &config,
                                stealth_config.clone(),
                                tabs.clone(),
                                browser_id_counter.clone(),
                            );
                            let _ = response.send(result);
                        }
                        CefCommand::CloseBrowser { tab_id, response } => {
                            let result = close_browser_internal(tab_id, tabs.clone());
                            let _ = response.send(result);
                        }
                        CefCommand::Navigate {
                            tab_id,
                            url,
                            response,
                        } => {
                            let result =
                                super::navigation::navigate_internal(tab_id, &url, tabs.clone());
                            let _ = response.send(result);
                        }
                        CefCommand::ExecuteJs {
                            tab_id,
                            script,
                            response,
                        } => {
                            let result = super::navigation::execute_js_internal(
                                tab_id,
                                &script,
                                tabs.clone(),
                            );
                            let _ = response.send(result);
                        }
                        CefCommand::ExecuteJsWithResult {
                            tab_id,
                            script,
                            response,
                        } => {
                            let result = super::navigation::execute_js_with_result_internal(tab_id, &script, tabs.clone());
                            let _ = response.send(result);
                        }
                        CefCommand::Screenshot {
                            tab_id,
                            options,
                            response,
                        } => {
                            let result = super::navigation::screenshot_internal(
                                tab_id,
                                &options,
                                tabs.clone(),
                            );
                            let _ = response.send(result);
                        }
                        CefCommand::MouseMove {
                            tab_id,
                            x,
                            y,
                            response,
                        } => {
                            let result =
                                super::input::mouse_move_internal(tab_id, x, y, tabs.clone());
                            let _ = response.send(result);
                        }
                        CefCommand::MouseClick {
                            tab_id,
                            x,
                            y,
                            button,
                            click_count,
                            response,
                        } => {
                            let result = super::input::mouse_click_internal(
                                tab_id,
                                x,
                                y,
                                button,
                                click_count,
                                tabs.clone(),
                            );
                            let _ = response.send(result);
                        }
                        CefCommand::MouseWheel {
                            tab_id,
                            x,
                            y,
                            delta_x,
                            delta_y,
                            response,
                        } => {
                            let result = super::input::mouse_wheel_internal(
                                tab_id,
                                x,
                                y,
                                delta_x,
                                delta_y,
                                tabs.clone(),
                            );
                            let _ = response.send(result);
                        }
                        CefCommand::KeyEvent {
                            tab_id,
                            event_type,
                            modifiers,
                            windows_key_code,
                            character,
                            response,
                        } => {
                            let result = super::input::key_event_internal(
                                tab_id,
                                event_type,
                                modifiers,
                                windows_key_code,
                                character,
                                tabs.clone(),
                            );
                            let _ = response.send(result);
                        }
                        CefCommand::TypeText {
                            tab_id,
                            text,
                            response,
                        } => {
                            let result =
                                super::input::type_text_internal(tab_id, &text, tabs.clone());
                            let _ = response.send(result);
                        }
                        CefCommand::Drag {
                            tab_id,
                            from_x,
                            from_y,
                            to_x,
                            to_y,
                            steps,
                            duration_ms,
                            response,
                        } => {
                            let result = super::input::drag_internal(tab_id, from_x, from_y, to_x, to_y, steps, duration_ms, tabs.clone());
                            let _ = response.send(result);
                        }
                        CefCommand::GoBack { tab_id, response } => {
                            let result = super::navigation::go_back_internal(tab_id, tabs.clone());
                            let _ = response.send(result);
                        }
                        CefCommand::GoForward { tab_id, response } => {
                            let result = super::navigation::go_forward_internal(tab_id, tabs.clone());
                            let _ = response.send(result);
                        }
                        CefCommand::ResizeViewport {
                            tab_id,
                            width,
                            height,
                            response,
                        } => {
                            let result = super::navigation::resize_viewport_internal(tab_id, width, height, tabs.clone());
                            let _ = response.send(result);
                        }
                        CefCommand::Shutdown { response } => {
                            info!("Processing shutdown command");

                            // Close all browsers
                            let tab_ids: Vec<Uuid> = {
                                let tabs_guard = tabs.read();
                                tabs_guard.keys().cloned().collect()
                            };

                            for tab_id in &tab_ids {
                                let _ = close_browser_internal(*tab_id, tabs.clone());
                            }

                            // Pump the CEF message loop so on_before_close callbacks
                            // can fire and CEF can clean up its internal browser_info_map.
                            // Without this, cef::shutdown() panics with "missing browser info map".
                            if !tab_ids.is_empty() {
                                info!("Pumping CEF message loop for browser cleanup ({} browsers)", tab_ids.len());
                                for _ in 0..50 {
                                    cef::do_message_loop_work();
                                    std::thread::sleep(std::time::Duration::from_millis(10));
                                }
                            }

                            is_running.store(false, Ordering::SeqCst);
                            let _ = response.send(Ok(()));
                            break 'main_loop;
                        }
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => {
                    // All commands drained, back to CEF work
                    break;
                }
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    warn!("Command channel disconnected");
                    break 'main_loop;
                }
            }
        }

        // Small delay to prevent CPU spinning
        std::thread::sleep(std::time::Duration::from_millis(CEF_MESSAGE_LOOP_DELAY_MS));
    }

    // Shutdown CEF
    info!("Shutting down CEF context");
    cef::shutdown();

    Ok(())
}

/// Creates a browser instance internally on the CEF thread.
///
/// Sets up the off-screen rendering handlers, life span and load handlers,
/// creates the CEF browser with the specified URL, and waits for the
/// `on_after_created` callback to fire before returning.
fn create_browser_internal(
    url: &str,
    tab_id: Uuid,
    config: &BrowserConfig,
    stealth_config: Arc<StealthConfig>,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    browser_id_counter: Arc<AtomicI32>,
) -> Result<()> {
    let viewport_dims = config.window_size;
    let viewport_size = Arc::new(RwLock::new(viewport_dims));

    // Create frame buffer for OSR
    let frame_buffer = Arc::new(RwLock::new(Vec::with_capacity(
        (viewport_dims.0 * viewport_dims.1 * 4) as usize,
    )));
    let frame_size = Arc::new(RwLock::new((0u32, 0u32)));
    let frame_version = Arc::new(AtomicU64::new(0));
    let browser_created = Arc::new(AtomicBool::new(false));

    // Create render handler using v144 wrap_render_handler! macro
    let render_handler = KiBrowserRenderHandlerImpl::new(
        tab_id,
        frame_buffer.clone(),
        frame_size.clone(),
        viewport_size.clone(),
        frame_version.clone(),
    );

    // Create life span handler with popup_tx for popup interception
    let life_span_handler = KiBrowserLifeSpanHandlerImpl::new(
        tab_id,
        tabs.clone(),
        browser_created.clone(),
        None, // popup_tx set later if needed
    );

    // Create load handler
    let load_handler = KiBrowserLoadHandlerImpl::new(
        tab_id,
        tabs.clone(),
        stealth_config.clone(),
    );

    // Create display handler (captures console.log for JS result communication)
    let display_handler = KiBrowserDisplayHandlerImpl::new(tab_id, tabs.clone());

    // Create client using v144 API
    let mut client = KiBrowserClient::new(
        tab_id,
        tabs.clone(),
        stealth_config.clone(),
        render_handler,
        life_span_handler,
        load_handler,
        display_handler,
    );

    // Browser settings
    let browser_settings = BrowserSettings {
        windowless_frame_rate: DEFAULT_FRAME_RATE,
        ..Default::default()
    };

    // Window info for OSR (off-screen rendering)
    let window_info = WindowInfo {
        bounds: Rect {
            x: 0,
            y: 0,
            width: viewport_dims.0 as i32,
            height: viewport_dims.1 as i32,
        },
        windowless_rendering_enabled: 1,
        ..Default::default()
    };

    // Create browser using v144 API
    let url_string = CefString::from(url);
    let result = cef::browser_host_create_browser(
        Some(&window_info),
        Some(&mut client),
        Some(&url_string),
        Some(&browser_settings),
        None,
        None,
    );

    if result == 0 {
        return Err(anyhow!("Failed to create CEF browser"));
    }

    // Store tab BEFORE browser creation (browser will be set in on_after_created)
    let cef_tab = CefTab::new(tab_id, url.to_string(), frame_buffer, frame_size, viewport_size, frame_version);
    tabs.write().insert(tab_id, cef_tab);

    // Wait for browser to be created (callback will be triggered)
    let start = std::time::Instant::now();
    while !browser_created.load(Ordering::SeqCst) {
        if start.elapsed() > std::time::Duration::from_secs(10) {
            // Remove the tab if browser creation failed
            tabs.write().remove(&tab_id);
            return Err(anyhow!("Timeout waiting for browser creation"));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        cef::do_message_loop_work();
    }

    browser_id_counter.fetch_add(1, Ordering::SeqCst);

    info!("Browser created for tab {} with URL: {}", tab_id, url);
    Ok(())
}

/// Closes a browser instance internally on the CEF thread.
///
/// Removes the tab from the shared map and requests the CEF browser host
/// to close the browser window.
pub(crate) fn close_browser_internal(
    tab_id: Uuid,
    tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
) -> Result<()> {
    let tab = {
        let mut tabs_guard = tabs.write();
        tabs_guard.remove(&tab_id)
    };

    if let Some(tab) = tab {
        // Close the browser
        if let Some(ref browser) = tab.browser {
            if let Some(host) = browser.host() {
                host.close_browser(1);
            }
        }
        info!("Browser closed for tab {}", tab_id);
        Ok(())
    } else {
        Err(anyhow!("Tab not found: {}", tab_id))
    }
}
