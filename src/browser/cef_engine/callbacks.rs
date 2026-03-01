//! CEF callback handler implementations for browser lifecycle and rendering.
//!
//! Contains the CEF v144 handler structs that receive callbacks from the Chromium
//! Embedded Framework: application startup, client routing, off-screen render
//! handler, life span handler for browser creation/closing, and load handler
//! for navigation events and stealth script injection.

use cef::{
    App, Browser, BrowserSettings, CefString, Client, Errorcode, Frame, LifeSpanHandler,
    LoadHandler, PaintElementType, Rect, RenderHandler, ScreenInfo, TransitionType, WindowInfo,
    ImplApp, ImplClient, ImplLifeSpanHandler, ImplLoadHandler, ImplRenderHandler,
    ImplBrowserHost, ImplCommandLine,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, error, info, trace};
use uuid::Uuid;

use crate::browser::tab::TabStatus;
use crate::stealth::StealthConfig;
use super::tab::CefTab;

/// Application handler for CEF lifecycle using v144 API.
///
/// Configures command line switches for stealth mode during CEF initialization,
/// disabling automation-detection features and GPU for headless stability.
#[cef::wrap_app]
pub(crate) struct KiBrowserApp {
    pub(crate) stealth_config: Arc<StealthConfig>,
}

impl ImplApp for KiBrowserApp {
    fn on_before_command_line_processing(
        &self,
        command_line: Option<&mut cef::CommandLine>,
    ) {
        if let Some(cmd) = command_line {
            // Add arguments for stealth mode
            cmd.append_switch_with_value(
                Some(&CefString::from("disable-blink-features")),
                Some(&CefString::from("AutomationControlled")),
            );
            cmd.append_switch(Some(&CefString::from("disable-infobars")));
            cmd.append_switch(Some(&CefString::from("disable-extensions")));
            cmd.append_switch(Some(&CefString::from("no-first-run")));
            cmd.append_switch(Some(&CefString::from("no-default-browser-check")));

            // Disable GPU in headless mode for stability
            cmd.append_switch(Some(&CefString::from("disable-gpu")));
            cmd.append_switch(Some(&CefString::from("disable-gpu-compositing")));

            debug!("CEF command line configured for stealth mode");
        }
    }
}

/// Client handler for browser events using v144 API.
///
/// Routes CEF callbacks to the appropriate sub-handlers for rendering,
/// life span management, and page loading events.
#[cef::wrap_client]
pub(crate) struct KiBrowserClient {
    pub(crate) tab_id: Uuid,
    pub(crate) tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    pub(crate) stealth_config: Arc<StealthConfig>,
    pub(crate) render_handler: RenderHandler,
    pub(crate) life_span_handler: LifeSpanHandler,
    pub(crate) load_handler: LoadHandler,
}

impl ImplClient for KiBrowserClient {
    fn get_render_handler(&self) -> Option<RenderHandler> {
        Some(self.render_handler.clone())
    }

    fn get_life_span_handler(&self) -> Option<LifeSpanHandler> {
        Some(self.life_span_handler.clone())
    }

    fn get_load_handler(&self) -> Option<LoadHandler> {
        Some(self.load_handler.clone())
    }
}

/// Render handler for off-screen rendering using v144 API.
///
/// Receives paint callbacks from CEF and stores the raw BGRA frame buffer
/// for later screenshot capture. Manages viewport geometry for the headless
/// browser window.
#[cef::wrap_render_handler]
pub(crate) struct KiBrowserRenderHandlerImpl {
    pub(crate) tab_id: Uuid,
    pub(crate) frame_buffer: Arc<RwLock<Vec<u8>>>,
    pub(crate) frame_size: Arc<RwLock<(u32, u32)>>,
    pub(crate) viewport_size: (u32, u32),
}

impl ImplRenderHandler for KiBrowserRenderHandlerImpl {
    fn get_view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) -> i32 {
        if let Some(r) = rect {
            r.x = 0;
            r.y = 0;
            r.width = self.viewport_size.0 as i32;
            r.height = self.viewport_size.1 as i32;
        }
        1 // Return true
    }

    fn get_screen_info(
        &self,
        _browser: Option<&mut Browser>,
        screen_info: Option<&mut ScreenInfo>,
    ) -> i32 {
        if let Some(info) = screen_info {
            info.device_scale_factor = 1.0;
            info.depth = 32;
            info.depth_per_component = 8;
            info.is_monochrome = 0;
            info.rect = Rect {
                x: 0,
                y: 0,
                width: self.viewport_size.0 as i32,
                height: self.viewport_size.1 as i32,
            };
            info.available_rect = Rect {
                x: 0,
                y: 0,
                width: self.viewport_size.0 as i32,
                height: self.viewport_size.1 as i32,
            };
        }
        1 // Return true
    }

    fn on_paint(
        &self,
        _browser: Option<&mut Browser>,
        element_type: PaintElementType,
        _dirty_rects: &[Rect],
        buffer: *const u8,
        width: i32,
        height: i32,
    ) {
        if element_type == PaintElementType::VIEW {
            // Store the frame buffer for screenshot capture
            let buffer_size = (width * height * 4) as usize;
            let buffer_slice = unsafe { std::slice::from_raw_parts(buffer, buffer_size) };

            let mut fb = self.frame_buffer.write();
            fb.clear();
            fb.extend_from_slice(buffer_slice);

            let mut size = self.frame_size.write();
            *size = (width as u32, height as u32);

            trace!(
                "Frame painted for tab {}: {}x{}, {} bytes",
                self.tab_id,
                width,
                height,
                buffer_size
            );
        }
    }
}

/// Life span handler for tab lifecycle events using v144 API.
///
/// Receives callbacks when a CEF browser instance is created or closed,
/// updating the shared tab map accordingly. Signals browser readiness
/// via the `browser_created` atomic flag.
#[cef::wrap_life_span_handler]
pub(crate) struct KiBrowserLifeSpanHandlerImpl {
    pub(crate) tab_id: Uuid,
    pub(crate) tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    pub(crate) browser_created: Arc<AtomicBool>,
}

impl ImplLifeSpanHandler for KiBrowserLifeSpanHandlerImpl {
    fn on_after_created(&self, browser: Option<&mut Browser>) {
        info!("Browser created for tab {}", self.tab_id);

        // Store browser reference in tab
        if let Some(b) = browser {
            let mut tabs = self.tabs.write();
            if let Some(tab) = tabs.get_mut(&self.tab_id) {
                tab.set_browser(b.clone());
            }
        }

        self.browser_created.store(true, Ordering::SeqCst);
    }

    fn on_before_close(&self, _browser: Option<&mut Browser>) {
        info!("Browser closing for tab {}", self.tab_id);
        let mut tabs = self.tabs.write();
        if let Some(tab) = tabs.get_mut(&self.tab_id) {
            tab.status = TabStatus::Closed;
            tab.browser = None;
        }
    }

    fn do_close(&self, _browser: Option<&mut Browser>) -> i32 {
        // Return 0 (false) to allow the browser to close
        0
    }
}

/// Load handler for navigation events and stealth injection using v144 API.
///
/// Injects stealth override scripts at page load start (before any page
/// scripts execute), tracks loading state changes, updates tab URLs on
/// load completion, and reports navigation errors.
#[cef::wrap_load_handler]
pub(crate) struct KiBrowserLoadHandlerImpl {
    pub(crate) tab_id: Uuid,
    pub(crate) tabs: Arc<RwLock<HashMap<Uuid, CefTab>>>,
    pub(crate) stealth_config: Arc<StealthConfig>,
}

impl ImplLoadHandler for KiBrowserLoadHandlerImpl {
    fn on_loading_state_change(
        &self,
        _browser: Option<&mut Browser>,
        is_loading: i32,
        can_go_back: i32,
        can_go_forward: i32,
    ) {
        let is_loading = is_loading != 0;
        let can_go_back = can_go_back != 0;
        let can_go_forward = can_go_forward != 0;

        let mut tabs = self.tabs.write();
        if let Some(tab) = tabs.get_mut(&self.tab_id) {
            if is_loading {
                tab.status = TabStatus::Loading;
                tab.is_ready.store(false, Ordering::SeqCst);
            } else {
                tab.status = TabStatus::Ready;
                tab.is_ready.store(true, Ordering::SeqCst);
            }
        }

        debug!(
            "Loading state changed for tab {}: loading={}, back={}, forward={}",
            self.tab_id, is_loading, can_go_back, can_go_forward
        );
    }

    fn on_load_start(
        &self,
        _browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        _transition_type: TransitionType,
    ) {
        if let Some(f) = frame {
            if f.is_main() != 0 {
                // Inject stealth scripts BEFORE any page scripts run
                let stealth_script = self.stealth_config.get_complete_override_script();
                let script_cef = CefString::from(stealth_script.as_str());
                let empty_url = CefString::from("");
                f.execute_java_script(Some(&script_cef), Some(&empty_url), 0);

                debug!(
                    "Stealth scripts injected for tab {} on load start",
                    self.tab_id
                );
            }
        }
    }

    fn on_load_end(
        &self,
        _browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        http_status_code: i32,
    ) {
        if let Some(f) = frame {
            if f.is_main() != 0 {
                // Update tab URL
                let mut tabs = self.tabs.write();
                if let Some(tab) = tabs.get_mut(&self.tab_id) {
                    let url = f.url();
                    // CefStringUserfreeUtf16 doesn't implement Display, use debug format
                    tab.url = format!("{:?}", url);
                }

                info!(
                    "Page loaded for tab {}: status={}",
                    self.tab_id, http_status_code
                );
            }
        }
    }

    fn on_load_error(
        &self,
        _browser: Option<&mut Browser>,
        frame: Option<&mut Frame>,
        error_code: Errorcode,
        error_text: Option<&CefString>,
        failed_url: Option<&CefString>,
    ) {
        if let Some(f) = frame {
            if f.is_main() != 0 {
                let url_str = failed_url.map(|u| u.to_string()).unwrap_or_default();
                let err_str = error_text.map(|e| e.to_string()).unwrap_or_default();

                let error_msg = format!(
                    "Failed to load {}: {:?} - {}",
                    url_str, error_code, err_str
                );

                let mut tabs = self.tabs.write();
                if let Some(tab) = tabs.get_mut(&self.tab_id) {
                    tab.status = TabStatus::Error(error_msg.clone());
                }

                error!("Load error for tab {}: {}", self.tab_id, error_msg);
            }
        }
    }
}
