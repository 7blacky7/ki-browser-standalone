//! KI-Browser Standalone - Main Entry Point
//!
//! This is the main executable for the ki-browser-standalone application.
//! It handles CLI argument parsing, configuration loading, and application startup.

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, Command};
use std::path::PathBuf;
use tokio::signal;
use tracing::{error, info, warn, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use ki_browser_standalone::{
    api::{ApiServer, IpcChannel},
    config::{BrowserSettings, CliArgs},
    stealth::StealthConfig, NAME, VERSION,
};

#[cfg(feature = "cef-browser")]
use ki_browser_standalone::browser::cef_headless::HeadlessRunner;

/// ANSI color codes for terminal output
#[allow(dead_code)]
mod colors {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";
    pub const CYAN: &str = "\x1b[36m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const BLUE: &str = "\x1b[34m";
    pub const MAGENTA: &str = "\x1b[35m";
}

/// Print the startup banner with version and ASCII art
fn print_banner() {
    println!(
        r#"
{cyan}{bold}  _  ___       ____
 | |/ (_)     | __ ) _ __ _____      _____  ___ _ __
 | ' /| |_____|  _ \| '__/ _ \ \ /\ / / __|/ _ \ '__|
 | . \| |_____| |_) | | | (_) \ V  V /\__ \  __/ |
 |_|\_\_|     |____/|_|  \___/ \_/\_/ |___/\___|_|
{reset}
{dim}  High-Performance Browser Automation with Stealth{reset}
{dim}  Version: {version}{reset}
"#,
        cyan = colors::CYAN,
        bold = colors::BOLD,
        reset = colors::RESET,
        dim = colors::DIM,
        version = VERSION
    );
}

/// Print configuration summary
fn print_config_summary(settings: &BrowserSettings, stealth_enabled: bool) {
    println!(
        "{bold}{blue}Configuration:{reset}",
        bold = colors::BOLD,
        blue = colors::BLUE,
        reset = colors::RESET
    );
    println!(
        "  {dim}Window Size:{reset}    {}x{}",
        settings.window_width,
        settings.window_height,
        dim = colors::DIM,
        reset = colors::RESET
    );
    println!(
        "  {dim}Headless:{reset}       {}",
        if settings.headless {
            format!("{green}yes{reset}", green = colors::GREEN, reset = colors::RESET)
        } else {
            format!("{yellow}no{reset}", yellow = colors::YELLOW, reset = colors::RESET)
        },
        dim = colors::DIM,
        reset = colors::RESET
    );
    println!(
        "  {dim}Stealth Mode:{reset}   {}",
        if stealth_enabled {
            format!("{green}enabled{reset}", green = colors::GREEN, reset = colors::RESET)
        } else {
            format!("{yellow}disabled{reset}", yellow = colors::YELLOW, reset = colors::RESET)
        },
        dim = colors::DIM,
        reset = colors::RESET
    );
    println!(
        "  {dim}API Server:{reset}     {}",
        if settings.api_enabled {
            format!(
                "{green}http://127.0.0.1:{}{reset}",
                settings.api_port,
                green = colors::GREEN,
                reset = colors::RESET
            )
        } else {
            format!("{yellow}disabled{reset}", yellow = colors::YELLOW, reset = colors::RESET)
        },
        dim = colors::DIM,
        reset = colors::RESET
    );
    println!(
        "  {dim}Max Tabs:{reset}       {}",
        settings.max_tabs,
        dim = colors::DIM,
        reset = colors::RESET
    );
    println!(
        "  {dim}Timeout:{reset}        {}ms",
        settings.default_timeout_ms,
        dim = colors::DIM,
        reset = colors::RESET
    );

    if let Some(ref proxy) = settings.proxy {
        println!(
            "  {dim}Proxy:{reset}          {}",
            proxy.to_url(),
            dim = colors::DIM,
            reset = colors::RESET
        );
    }

    if let Some(ref profile) = settings.profile_path {
        println!(
            "  {dim}Profile:{reset}        {}",
            profile.display(),
            dim = colors::DIM,
            reset = colors::RESET
        );
    }

    println!();
}

/// Build the CLI command parser
fn build_cli() -> Command {
    Command::new(NAME)
        .version(VERSION)
        .author("KI-Browser Team")
        .about("High-performance browser automation with built-in stealth capabilities")
        .long_about(
            "KI-Browser Standalone is a browser automation tool featuring:\n\
             - Human-like input simulation\n\
             - Anti-detection and fingerprint spoofing\n\
             - REST API for remote control\n\
             - WebSocket event streaming\n\
             - Flexible configuration options",
        )
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Path to configuration file (TOML or JSON)")
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("port")
                .short('p')
                .long("port")
                .value_name("PORT")
                .help("API server port (default: 9222)")
                .value_parser(clap::value_parser!(u16)),
        )
        .arg(
            Arg::new("headless")
                .long("headless")
                .help("Run browser in headless mode")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-headless")
                .long("no-headless")
                .help("Run browser with visible window")
                .action(ArgAction::SetTrue)
                .conflicts_with("headless"),
        )
        .arg(
            Arg::new("stealth")
                .short('s')
                .long("stealth")
                .help("Enable stealth mode for anti-detection")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-stealth")
                .long("no-stealth")
                .help("Disable stealth mode")
                .action(ArgAction::SetTrue)
                .conflicts_with("stealth"),
        )
        .arg(
            Arg::new("no-api")
                .long("no-api")
                .help("Disable the REST API server")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("width")
                .long("width")
                .value_name("PIXELS")
                .help("Browser window width")
                .value_parser(clap::value_parser!(u32)),
        )
        .arg(
            Arg::new("height")
                .long("height")
                .value_name("PIXELS")
                .help("Browser window height")
                .value_parser(clap::value_parser!(u32)),
        )
        .arg(
            Arg::new("user-agent")
                .long("user-agent")
                .value_name("STRING")
                .help("Custom user agent string"),
        )
        .arg(
            Arg::new("profile")
                .long("profile")
                .value_name("PATH")
                .help("Path to browser profile directory")
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("max-tabs")
                .long("max-tabs")
                .value_name("COUNT")
                .help("Maximum number of concurrent tabs")
                .value_parser(clap::value_parser!(usize)),
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .value_name("MS")
                .help("Default operation timeout in milliseconds")
                .value_parser(clap::value_parser!(u64)),
        )
        .arg(
            Arg::new("cdp-port")
                .long("cdp-port")
                .value_name("PORT")
                .help("CDP remote debugging port (default: 9222, 0 to disable)")
                .value_parser(clap::value_parser!(u16)),
        )
        .arg(
            Arg::new("proxy")
                .long("proxy")
                .value_name("HOST:PORT")
                .help("Proxy server address (e.g., localhost:8080)"),
        )
        .arg(
            Arg::new("proxy-type")
                .long("proxy-type")
                .value_name("TYPE")
                .help("Proxy type: http, https, or socks5")
                .value_parser(["http", "https", "socks5"]),
        )
        .arg(
            Arg::new("proxy-auth")
                .long("proxy-auth")
                .value_name("USER:PASS")
                .help("Proxy authentication credentials"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Enable verbose logging")
                .action(ArgAction::Count),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .help("Suppress output except errors")
                .action(ArgAction::SetTrue)
                .conflicts_with("verbose"),
        )
        .arg(
            Arg::new("gui")
                .long("gui")
                .help("Start with custom GUI browser (CEF-based)")
                .action(ArgAction::SetTrue),
        )
}

/// Parse CLI arguments into CliArgs struct
fn parse_cli_args(matches: &clap::ArgMatches) -> CliArgs {
    let mut args = CliArgs {
        config_file: matches.get_one::<PathBuf>("config").cloned(),
        api_port: matches.get_one::<u16>("port").copied(),
        width: matches.get_one::<u32>("width").copied(),
        height: matches.get_one::<u32>("height").copied(),
        user_agent: matches.get_one::<String>("user-agent").cloned(),
        profile_path: matches.get_one::<PathBuf>("profile").cloned(),
        max_tabs: matches.get_one::<usize>("max-tabs").copied(),
        timeout_ms: matches.get_one::<u64>("timeout").copied(),
        cdp_port: matches.get_one::<u16>("cdp-port").copied(),
        ..Default::default()
    };

    // Handle headless flag
    if matches.get_flag("headless") {
        args.headless = Some(true);
    } else if matches.get_flag("no-headless") {
        args.headless = Some(false);
    }

    // Handle stealth flag
    if matches.get_flag("stealth") {
        args.stealth_mode = Some(true);
    } else if matches.get_flag("no-stealth") {
        args.stealth_mode = Some(false);
    }

    // Handle no-api flag
    if matches.get_flag("no-api") {
        args.api_enabled = Some(false);
    }

    // Parse proxy settings
    if let Some(proxy) = matches.get_one::<String>("proxy") {
        let parts: Vec<&str> = proxy.rsplitn(2, ':').collect();
        if parts.len() == 2 {
            if let Ok(port) = parts[0].parse::<u16>() {
                args.proxy_host = Some(parts[1].to_string());
                args.proxy_port = Some(port);
            }
        } else {
            args.proxy_host = Some(proxy.clone());
        }
    }

    args.proxy_type = matches.get_one::<String>("proxy-type").cloned();

    if let Some(auth) = matches.get_one::<String>("proxy-auth") {
        let parts: Vec<&str> = auth.splitn(2, ':').collect();
        if parts.len() == 2 {
            args.proxy_username = Some(parts[0].to_string());
            args.proxy_password = Some(parts[1].to_string());
        } else {
            args.proxy_username = Some(auth.clone());
        }
    }

    args
}

/// Initialize the tracing/logging subsystem
fn init_tracing(verbosity: u8, quiet: bool) {
    let level = if quiet {
        Level::ERROR
    } else {
        match verbosity {
            0 => Level::INFO,
            1 => Level::DEBUG,
            _ => Level::TRACE,
        }
    };

    let filter = EnvFilter::from_default_env()
        .add_directive(level.into())
        .add_directive("hyper=warn".parse().unwrap())
        .add_directive("tower_http=info".parse().unwrap());

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(true).with_thread_ids(false))
        .with(filter)
        .init();
}

/// Initialize stealth configuration if enabled
fn init_stealth(settings: &BrowserSettings) -> Option<StealthConfig> {
    if settings.stealth_mode {
        let mut config = StealthConfig::random_chrome();
        // Sync screen resolution to the actual viewport so that
        // screen.width >= outerWidth >= innerWidth and orientation is correct.
        config.sync_screen_to_viewport(settings.window_width, settings.window_height);
        if let Err(e) = config.validate() {
            warn!("Stealth configuration validation warning: {}", e);
        }
        info!(
            "Stealth mode initialized with random fingerprint (screen synced to {}x{} viewport)",
            settings.window_width, settings.window_height
        );
        Some(config)
    } else {
        None
    }
}

/// Main application entry point
#[tokio::main]
async fn main() -> Result<()> {
    // CEF subprocess handling: CEF launches sub-processes (renderer, GPU, etc.)
    // using the same executable with --type=xxx arguments. We must handle these
    // BEFORE clap parsing, because clap doesn't know about CEF's arguments.
    #[cfg(feature = "cef-browser")]
    {
        // Check if this is a CEF subprocess by looking for --type in raw args
        let args: Vec<String> = std::env::args().collect();
        let is_cef_subprocess = args.iter().any(|a| a.starts_with("--type=") || a == "--type");

        if is_cef_subprocess {
            use cef::{api_hash, execute_process, MainArgs, sys};
            let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
            let main_args = MainArgs::default();
            let exit_code = execute_process(Some(&main_args), None, std::ptr::null_mut());
            std::process::exit(exit_code);
        }

        // Note: GPU flags are set conditionally in KiBrowserApp::on_before_command_line_processing
        // based on headless vs GUI mode. No blanket CHROME_FLAGS needed here.
    }

    // Parse CLI arguments
    let matches = build_cli().get_matches();

    // Get verbosity settings before loading config
    let verbosity = matches.get_count("verbose");
    let quiet = matches.get_flag("quiet");

    // Initialize logging
    init_tracing(verbosity, quiet);

    // Convert matches to CliArgs
    let cli_args = parse_cli_args(&matches);
    #[cfg_attr(not(feature = "gui"), allow(unused_variables))]
    let use_gui = matches.get_flag("gui");

    // Load configuration with full precedence chain
    let settings = cli_args
        .load_settings()
        .context("Failed to load configuration")?;

    // Print banner unless quiet mode
    if !quiet {
        print_banner();
        print_config_summary(&settings, settings.stealth_mode);
    }

    // Initialize stealth configuration if enabled
    let _stealth_config = init_stealth(&settings);

    // GUI mode: start CEF-based GUI browser
    #[cfg(feature = "gui")]
    if use_gui {
        use std::sync::Arc;
        use ki_browser_standalone::browser::BrowserConfig;
        use ki_browser_standalone::api::{ApiServer, IpcChannel};
        use ki_browser_standalone::gui::GuiHandle;

        info!("Starting GUI browser mode...");

        let mut browser_config = BrowserConfig::new()
            .headless(false)
            .window_size(settings.window_width, settings.window_height)
            .cdp_port(settings.cdp_port);

        // Pass stealth config to CEF engine — ensures ONE identity.
        if let Some(ref stealth) = _stealth_config {
            browser_config.stealth_config = Some(stealth.clone());
            browser_config = browser_config.user_agent(&stealth.fingerprint.user_agent);
        } else if let Some(ref ua) = settings.user_agent {
            browser_config = browser_config.user_agent(ua);
        }

        if let Some(ref proxy) = settings.proxy {
            browser_config = browser_config.proxy(proxy.to_url());
        }

        let api_port = settings.api_port;

        // Create the shared GUI handle BEFORE engine and server so both
        // can hold a reference for visibility control and shutdown signaling.
        let gui_handle = GuiHandle::new();

        // Create CEF engine FIRST -- needed by both API handler and GUI.
        use ki_browser_standalone::browser::BrowserEngine;
        let engine = ki_browser_standalone::browser::cef_engine::CefBrowserEngine::new(browser_config).await?;
        let engine = Arc::new(engine);

        // Start API server in background if enabled.
        let mut api_server = if settings.api_enabled {
            let ipc_channel = IpcChannel::new();
            let mut handler = ki_browser_standalone::api::BrowserCommandHandler::with_cef_shared(engine.clone());

            // Wire CDP client for privileged JS evaluation (bypasses CSP/Trusted Types)
            if let Some(cdp_port) = settings.cdp_port {
                let cdp_client = std::sync::Arc::new(ki_browser_standalone::api::cdp_client::CdpClient::new(cdp_port));
                handler.set_cdp_client(cdp_client);
                info!("CDP client enabled on port {} for CSP-bypass evaluation", cdp_port);
            }

            // Pass complete stealth script for CDP pre-document injection
            if let Some(ref stealth) = _stealth_config {
                handler.set_stealth_init_script(stealth.get_complete_override_script());
                info!("Stealth init-script set for CDP injection (WebGL, navigator, canvas, etc.)");
            }

            let ipc_channel_clone = ipc_channel.clone();
            tokio::spawn(async move {
                if let Some(mut processor) = ki_browser_standalone::api::IpcProcessor::new(&ipc_channel_clone).await {
                    handler.run(&mut processor).await;
                }
            });

            let mut server = ApiServer::new_with_cdp(api_port, ipc_channel, settings.cdp_port);
            // Store GuiHandle in AppState so GUI toggle endpoints can use it.
            server.state_mut().set_gui_handle(gui_handle.clone());
            // Store CefEngine reference for /ws/viewer frame-buffer access.
            server.state_mut().set_cef_engine(engine.clone());

            server
                .start()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to start API server: {}", e))?;

            info!("API server started on port {} (connected to CEF engine)", api_port);
            Some(server)
        } else {
            None
        };

        // Spawn a background task that forwards SIGINT/SIGTERM to the GUI handle
        // so the event loop exits gracefully instead of being killed mid-frame.
        let shutdown_handle = gui_handle.clone();
        tokio::spawn(async move {
            if signal::ctrl_c().await.is_ok() {
                info!("Received shutdown signal, requesting GUI shutdown...");
                shutdown_handle.request_shutdown();
            }
        });

        // IMPORTANT: run_gui() MUST be called from the main thread (X11/Wayland).
        // The tokio runtime worker threads keep the API server running in the
        // background while the GUI event loop blocks the main thread.
        let gui_result = ki_browser_standalone::gui::run_gui(engine, api_port, gui_handle);

        // GUI has exited -- stop the API server gracefully.
        if let Some(ref mut server) = api_server {
            info!("GUI closed, stopping API server...");
            server.stop().await;
        }

        info!("KI-Browser stopped successfully.");
        return gui_result;
    }

    // Initialize browser engine
    info!("Initializing browser engine...");

    // CEF browser engine (headless, no GUI) — managed by HeadlessRunner
    #[cfg(feature = "cef-browser")]
    let (_cef_engine, _headless_runner) = {
        use std::sync::Arc;
        use ki_browser_standalone::browser::{BrowserConfig, CefBrowserEngine, BrowserEngine};

        let mut browser_config = BrowserConfig::new()
            .headless(settings.headless)
            .window_size(settings.window_width, settings.window_height)
            .cdp_port(settings.cdp_port);

        // Pass stealth config to CEF engine — ensures ONE identity for
        // HTTP headers, JS navigator, and all tabs.
        if let Some(ref stealth) = _stealth_config {
            info!("Stealth identity: {}", stealth.fingerprint.user_agent);
            browser_config.stealth_config = Some(stealth.clone());
            browser_config = browser_config.user_agent(&stealth.fingerprint.user_agent);
        } else if let Some(ref ua) = settings.user_agent {
            browser_config = browser_config.user_agent(ua);
        }

        if let Some(ref proxy) = settings.proxy {
            browser_config = browser_config.proxy(proxy.to_url());
        }

        let engine = CefBrowserEngine::new(browser_config).await
            .context("Failed to initialize CEF browser engine")?;
        let engine = Arc::new(engine);
        let runner = HeadlessRunner::new(engine.clone());
        runner.start().await.map_err(|e| anyhow::anyhow!("Failed to start headless runner: {}", e))?;
        info!("CEF headless mode active");
        (engine, runner)
    };

    #[cfg(not(feature = "cef-browser"))]
    {
        warn!("No browser feature enabled — all browser commands will return errors");
    }

    // Start API server if enabled
    let mut api_server = if settings.api_enabled {
        info!("Starting API server on port {}...", settings.api_port);

        let ipc_channel = IpcChannel::new();

        // Set up browser command handler with the actual browser engine
        #[cfg(feature = "cef-browser")]
        let handler = {
            info!("Browser handler configured with CEF engine");
            let mut h = ki_browser_standalone::api::BrowserCommandHandler::with_cef_shared(_cef_engine.clone());
            // Wire CDP client for privileged JS evaluation (bypasses CSP/Trusted Types)
            if let Some(cdp_port) = settings.cdp_port {
                let cdp_client = std::sync::Arc::new(ki_browser_standalone::api::cdp_client::CdpClient::new(cdp_port));
                h.set_cdp_client(cdp_client);
                info!("CDP client enabled on port {} for CSP-bypass evaluation", cdp_port);
            }
            // Pass complete stealth script for CDP pre-document injection
            if let Some(ref stealth) = _stealth_config {
                h.set_stealth_init_script(stealth.get_complete_override_script());
            }
            h
        };

        #[cfg(not(feature = "cef-browser"))]
        let handler = ki_browser_standalone::api::BrowserCommandHandler::new();

        // Start IPC processor in background
        let ipc_channel_clone = ipc_channel.clone();
        tokio::spawn(async move {
            if let Some(mut processor) = ki_browser_standalone::api::IpcProcessor::new(&ipc_channel_clone).await {
                handler.run(&mut processor).await;
            }
        });

        let mut server = ApiServer::new_with_cdp(settings.api_port, ipc_channel, settings.cdp_port);

        // Store CefEngine reference for /ws/viewer frame-buffer access.
        #[cfg(feature = "cef-browser")]
        server.state_mut().set_cef_engine(_cef_engine.clone());

        server
            .start()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to start API server: {}", e))?;

        println!(
            "{green}{bold}API Server started:{reset} http://127.0.0.1:{}",
            settings.api_port,
            green = colors::GREEN,
            bold = colors::BOLD,
            reset = colors::RESET
        );
        if !settings.headless {
            println!(
                "{green}{bold}Dashboard:{reset}    http://127.0.0.1:{}/ui",
                settings.api_port,
                green = colors::GREEN,
                bold = colors::BOLD,
                reset = colors::RESET
            );
        }
        if let Some(cdp_port) = settings.cdp_port {
            println!(
                "{green}{bold}CDP Debugging:{reset}  http://127.0.0.1:{}/json/list",
                cdp_port,
                green = colors::GREEN,
                bold = colors::BOLD,
                reset = colors::RESET
            );
        }
        println!(
            "{dim}Press Ctrl+C to stop{reset}",
            dim = colors::DIM,
            reset = colors::RESET
        );
        println!();

        Some(server)
    } else {
        info!("API server disabled");
        None
    };

    // Wait for shutdown signal
    info!("KI-Browser is running. Press Ctrl+C to stop.");

    match signal::ctrl_c().await {
        Ok(()) => {
            println!();
            info!("Received shutdown signal, stopping gracefully...");
        }
        Err(e) => {
            error!("Failed to listen for shutdown signal: {}", e);
        }
    }

    // Graceful shutdown
    if let Some(ref mut server) = api_server {
        info!("Stopping API server...");
        server.stop().await;
    }

    // Browser engine cleanup is handled by HeadlessRunner/CefBrowserEngine Drop impls

    println!(
        "{green}KI-Browser stopped successfully.{reset}",
        green = colors::GREEN,
        reset = colors::RESET
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        let cmd = build_cli();

        // Test basic parsing
        let matches = cmd
            .clone()
            .try_get_matches_from(["ki-browser", "--headless", "--stealth"])
            .unwrap();

        assert!(matches.get_flag("headless"));
        assert!(matches.get_flag("stealth"));
    }

    #[test]
    fn test_cli_port_parsing() {
        let cmd = build_cli();

        let matches = cmd
            .clone()
            .try_get_matches_from(["ki-browser", "--port", "8080"])
            .unwrap();

        assert_eq!(matches.get_one::<u16>("port"), Some(&8080));
    }

    #[test]
    fn test_cli_conflicts() {
        let cmd = build_cli();

        // headless and no-headless should conflict
        let result = cmd
            .clone()
            .try_get_matches_from(["ki-browser", "--headless", "--no-headless"]);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_cli_args() {
        let cmd = build_cli();
        let matches = cmd
            .try_get_matches_from([
                "ki-browser",
                "--headless",
                "--port",
                "9000",
                "--width",
                "1920",
                "--height",
                "1080",
            ])
            .unwrap();

        let args = parse_cli_args(&matches);

        assert_eq!(args.headless, Some(true));
        assert_eq!(args.api_port, Some(9000));
        assert_eq!(args.width, Some(1920));
        assert_eq!(args.height, Some(1080));
    }
}
