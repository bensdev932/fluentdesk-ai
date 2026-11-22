//! Desktop and headless-bridge entrypoint for FluentDesk AI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod desktop;

use fluentdesk_core::local_server::{run_default_bridge, DEFAULT_SERVER_PORT};

fn main() {
    if should_run_headless() {
        run_headless_cli();
    } else {
        desktop::run();
    }
}

fn should_run_headless() -> bool {
    std::env::args().skip(1).any(|arg| arg == "--headless")
}

fn run_headless_cli() {
    fluentdesk_core::telemetry::init_default_tracing();
    let port = resolve_port_from_args_or_env();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build();
    let res = rt
        .map_err(|e| e.to_string())
        .and_then(|r| r.block_on(run_default_bridge(port)));
    if let Err(err) = res {
        tracing::error!(error = %err, "FluentDesk AI headless backend exited");
        std::process::exit(1);
    }
}

fn resolve_port_from_args_or_env() -> u16 {
    if let Some(arg_port) = parse_cli_port() {
        return arg_port;
    }
    std::env::var("FLUENTDESK_SERVER_PORT")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .filter(|&p| p > 0)
        .unwrap_or(DEFAULT_SERVER_PORT)
}

fn parse_cli_port() -> Option<u16> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--port" {
            return args
                .next()
                .and_then(|v| v.parse::<u16>().ok())
                .filter(|&p| p > 0);
        }
        if let Some(val) = arg.strip_prefix("--port=") {
            return val.parse::<u16>().ok().filter(|&p| p > 0);
        }
    }
    None
}
