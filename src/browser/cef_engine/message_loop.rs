//! CEF message loop, initialization, and browser instance creation.
//!
//! Contains the synchronous message loop that runs on a dedicated thread,
//! processes CEF work and dispatches commands from the async API. Also
//! provides internal functions for creating and closing browser instances
//! on the CEF thread where single-threaded access is required.

use anyhow::{anyhow, Result};
use cef::{
    App, BrowserSettings, CefString, Client, LifeSpanHandler, LoadHandler, MainArgs, Rect,
    RenderHandler, Settings, WindowInfo, LogSeverity,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::browser::engine::BrowserConfig;
use crate::stealth::StealthConfig;
use super::callbacks::{
    KiBrowserApp, KiBrowserClient, KiBrowserLifeSpanHandlerImpl, KiBrowserLoadHandlerImpl,
    KiBrowserRenderHandlerImpl,
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
    mut command_rx: mpsc::Receiver<CefCommand>,
) -> Result<()> {
    // Configure CEF settings
    let mut settings = Settings::default();
    settings.windowless_rendering_enabled = 1;
    settings.no_sandbox = 1;
    settings.multi_threaded_message_loop = 0;
    settings.external_message_pump = 1;

    if config.headless {
        settings.windowless_rendering_enabled = 1;
    }

    // Set user agent if provided
    if let Some(ref user_agent) = config.user_agent {
        settings.user_agent = CefString::from(user_agent.as_str());
    }

    // Set log level
    settings.log_severity = LogSeverity::WARNING;

    // Create app with v144 API
    let app_impl = KiBrowserApp {
        stealth_config: stealth_config.clone(),
    };
    let mut app = App::new(app_impl);

    // Create main args
    let args = MainArgs::default();

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
    loop {
        // Process CEF work
        cef::do_message_loop_work();

        // Process commands with timeout
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
                    CefCommand::Shutdown { response } => {
                        info!("Processing shutdown command");

                        // Close all browsers
                        let tab_ids: Vec<Uuid> = {
                            let tabs_guard = tabs.read();
                            tabs_guard.keys().cloned().collect()
                        };

                        for tab_id in tab_ids {
                            let _ = close_browser_internal(tab_id, tabs.clone());
                        }

                        is_running.store(false, Ordering::SeqCst);
                        let _ = response.send(Ok(()));
                        break;
                    }
                }
            }
            Err(mpsc::error::TryRecvError::Empty) => {
                // No command, continue message loop
            }
            Err(mpsc::error::TryRecvError::Disconnected) => {
                warn!("Command channel disconnected");
                break;
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
    let viewport_size = config.window_size;

    // Create frame buffer for OSR
    let frame_buffer = Arc::new(RwLock::new(Vec::with_capacity(
        (viewport_size.0 * viewport_size.1 * 4) as usize,
    )));
    let frame_size = Arc::new(RwLock::new((0u32, 0u32)));
    let browser_created = Arc::new(AtomicBool::new(false));

    // Create render handler using v144 API
    let render_handler_impl = KiBrowserRenderHandlerImpl {
        tab_id,
        frame_buffer: frame_buffer.clone(),
        frame_size: frame_size.clone(),
        viewport_size,
    };
    let render_handler = RenderHandler::new(render_handler_impl);

    // Create life span handler
    let life_span_handler_impl = KiBrowserLifeSpanHandlerImpl {
        tab_id,
        tabs: tabs.clone(),
        browser_created: browser_created.clone(),
    };
    let life_span_handler = LifeSpanHandler::new(life_span_handler_impl);

    // Create load handler
    let load_handler_impl = KiBrowserLoadHandlerImpl {
        tab_id,
        tabs: tabs.clone(),
        stealth_config: stealth_config.clone(),
    };
    let load_handler = LoadHandler::new(load_handler_impl);

    // Create client using v144 API
    let client_impl = KiBrowserClient {
        tab_id,
        tabs: tabs.clone(),
        stealth_config: stealth_config.clone(),
        render_handler,
        life_span_handler,
        load_handler,
    };
    let mut client = Client::new(client_impl);

    // Browser settings
    let mut browser_settings = BrowserSettings::default();
    browser_settings.windowless_frame_rate = DEFAULT_FRAME_RATE;

    // Window info for OSR (off-screen rendering)
    let mut window_info = WindowInfo::default();
    window_info.bounds = Rect {
        x: 0,
        y: 0,
        width: viewport_size.0 as i32,
        height: viewport_size.1 as i32,
    };
    window_info.windowless_rendering_enabled = 1;

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
    let cef_tab = CefTab::new(tab_id, url.to_string(), frame_buffer, frame_size);
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
