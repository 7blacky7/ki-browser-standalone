//! ki-browser-client — lightweight viewer for remote ki-browser instances.
//!
//! Connects via WebSocket to a ki-browser server's /ws/viewer endpoint,
//! displays streamed JPEG frames in an egui/wgpu window, and forwards
//! mouse/keyboard input back to the server.

mod app;
mod connection;
mod protocol;

use clap::Parser;
use tracing::info;

/// Lightweight viewer client for ki-browser remote streaming.
#[derive(Parser)]
#[command(name = "ki-browser-client", version, about)]
struct Cli {
    /// Server WebSocket URL (e.g. ws://192.168.50.65:3000/ws/viewer)
    #[arg(short, long, default_value = "ws://127.0.0.1:3000/ws/viewer")]
    url: String,

    /// Window width
    #[arg(long, default_value = "1280")]
    width: u32,

    /// Window height
    #[arg(long, default_value = "800")]
    height: u32,
}

fn main() {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ki_browser_client=info,warn".into()),
        )
        .init();

    info!("ki-browser-client starting, connecting to {}", cli.url);

    // Create tokio runtime for async WebSocket communication.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([cli.width as f32, cli.height as f32])
            .with_title("ki-browser viewer"),
        ..Default::default()
    };

    let rt_handle = runtime.handle().clone();
    let url = cli.url.clone();

    eframe::run_native(
        "ki-browser-client",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(app::ViewerApp::new(url, rt_handle)))
        }),
    )
    .expect("Failed to run eframe");
}
