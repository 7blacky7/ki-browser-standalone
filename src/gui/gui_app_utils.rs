//! GUI application utilities: single-instance PID file lock and eframe launcher.
//!
//! Provides `acquire_instance_lock` for single-instance enforcement via a PID
//! file under `/tmp`, and `run_gui` which is the public entry point that
//! configures eframe options and starts the native event loop. Extracted from
//! `browser_app.rs` to keep the main application struct focused on state and
//! the eframe::App trait implementation.

use std::sync::Arc;

use tracing::{info, warn};

use crate::browser::cef_engine::CefBrowserEngine;

use super::browser_app::KiBrowserApp;
use super::handle::GuiHandle;

/// PID file path for single-instance enforcement.
const PID_FILE: &str = "/tmp/ki-browser-gui.pid";

/// Check if another GUI instance is already running via PID file.
///
/// Creates a PID file with the current process ID. If a PID file already
/// exists and the referenced process is still alive, returns an error to
/// prevent multiple GUI instances from competing for window resources.
pub(super) fn acquire_instance_lock() -> anyhow::Result<()> {
    use std::io::Read;

    if let Ok(mut f) = std::fs::File::open(PID_FILE) {
        let mut contents = String::new();
        if f.read_to_string(&mut contents).is_ok() {
            if let Ok(pid) = contents.trim().parse::<u32>() {
                let proc_path = format!("/proc/{}", pid);
                if std::path::Path::new(&proc_path).exists() {
                    return Err(anyhow::anyhow!(
                        "Another KI-Browser GUI instance is already running (PID {}). \
                         Kill it first or remove {}",
                        pid, PID_FILE
                    ));
                }
            }
        }
        warn!("Removing stale PID file");
        let _ = std::fs::remove_file(PID_FILE);
    }

    std::fs::write(PID_FILE, std::process::id().to_string())
        .map_err(|e| anyhow::anyhow!("Failed to write PID file: {}", e))?;

    Ok(())
}

/// Remove the PID file (used during shutdown cleanup).
pub(super) fn remove_pid_file() {
    let _ = std::fs::remove_file(PID_FILE);
}

/// Starts the GUI browser. MUST be called from the main thread (X11/Wayland requirement).
/// Blocks until the GUI window is closed or a shutdown is requested.
///
/// The `gui_handle` parameter is created by `GuiHandle::new()` and should be
/// shared with the API server *before* calling this function so that REST
/// endpoints can control visibility and request shutdown.
pub fn run_gui(
    engine: Arc<CefBrowserEngine>,
    api_port: u16,
    gui_handle: Arc<GuiHandle>,
) -> anyhow::Result<()> {
    acquire_instance_lock()?;

    info!("Starting GUI browser window");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("KI-Browser")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_decorations(false),
        ..Default::default()
    };

    let app = KiBrowserApp::new(engine, api_port, gui_handle.clone());

    let result = eframe::run_native(
        "KI-Browser",
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    ).map_err(|e| anyhow::anyhow!("GUI error: {}", e));

    // Ensure cleanup even if on_exit was not called (e.g. panic)
    remove_pid_file();
    gui_handle.mark_shutdown_complete();

    result
}
