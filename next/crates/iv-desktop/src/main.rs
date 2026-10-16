#![forbid(unsafe_code)]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod client;
mod engine;
mod protocol;
mod run_export;
mod ui;

use protocol::Command;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

fn argument(args: &[String], key: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == key).map(|w| w[1].clone())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let boot = Instant::now();
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--default-config") {
        println!(
            "{}",
            serde_json::to_string(&iv_core::RunConfig::default()).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if args.iter().any(|a| a == "--engine") {
        return engine::serve();
    }
    if args.iter().any(|a| a == "--list") {
        let paths = iv_transport::HelperPaths::discover()?;
        let result = iv_transport::discover_resources(&paths);
        println!(
            "{}",
            serde_json::json!({"resources":result.resources,"warnings":result.warnings})
        );
        return Ok(());
    }
    if args.iter().any(|a| a == "--identify") {
        let paths = iv_transport::HelperPaths::discover()?;
        let source = argument(&args, "--source").ok_or("--source is required")?;
        let meter = argument(&args, "--meter").ok_or("--meter is required")?;
        let mut pair = iv_transport::DevicePair::open(&paths, &source, &meter)?;
        let output = pair.query(iv_transport::Role::Source, ":OUTP?")?;
        println!(
            "{}",
            serde_json::json!({"source":pair.source_identity,"meter":pair.meter_identity,"output":output})
        );
        pair.close()?;
        return Ok(());
    }
    if args.iter().any(|a| a == "--measure") {
        return command_measure(&args);
    }
    if args.iter().any(|a| a == "--version") {
        println!("KohdaLab IV Next {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if cfg!(windows) {
        let home = std::env::var_os("USERPROFILE").ok_or("Windows user profile was not found")?;
        let working = std::path::PathBuf::from(home).join(".kohdalab");
        std::fs::create_dir_all(&working)
            .map_err(|e| format!("Create user data directory: {e}"))?;
        std::env::set_current_dir(working).map_err(|e| format!("Open user data directory: {e}"))?;
    }
    let hardware_smoke = args.iter().any(|a| a == "--gui-hardware-smoke");
    let hardware = if hardware_smoke {
        if !args.iter().any(|a| a == "--confirm-open-circuit") {
            return Err("Hardware GUI smoke requires --confirm-open-circuit".into());
        }
        Some((
            argument(&args, "--source").ok_or("--source is required")?,
            argument(&args, "--meter").ok_or("--meter is required")?,
        ))
    } else {
        None
    };
    let smoke = hardware_smoke || args.iter().any(|a| a == "--gui-smoke");
    let screenshot = argument(&args, "--screenshot").map(std::path::PathBuf::from);
    let icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/kohdalab-iv-window.png"))
            .map_err(|error| format!("Load application icon: {error}"))?;
    let options = eframe::NativeOptions {
        vsync: false,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_icon(icon)
            .with_inner_size([1240.0, 760.0])
            .with_min_inner_size([1080.0, 650.0]),
        ..Default::default()
    };
    let success = Arc::new(AtomicBool::new(false));
    let gui_success = success.clone();
    eframe::run_native(
        concat!("KohdaLab IV v", env!("CARGO_PKG_VERSION")),
        options,
        Box::new(move |cc| {
            Ok(Box::new(ui::IvApp::new(
                cc,
                smoke,
                screenshot,
                gui_success,
                boot,
                hardware,
            )))
        }),
    )
    .map_err(|e| e.to_string())?;
    if smoke && !success.load(Ordering::Acquire) {
        return Err("GUI smoke did not complete successfully".into());
    }
    Ok(())
}

fn command_measure(args: &[String]) -> Result<(), String> {
    let simulation = args.iter().any(|a| a == "--simulate");
    if !simulation && !args.iter().any(|a| a == "--confirm-open-circuit") {
        return Err("Hardware CLI preview requires --confirm-open-circuit and is limited to +/-1 mV, 1 mA. Use the GUI for a reviewed custom plan.".into());
    }
    let source = argument(args, "--source").unwrap_or_default();
    let meter = argument(args, "--meter").unwrap_or_default();
    let path = argument(args, "--output").ok_or("--output CSV path is required")?;
    let mut config = iv_core::RunConfig {
        pattern: iv_core::Pattern::Linear,
        step: 0.001,
        ..Default::default()
    };
    if simulation {
        config.start_settle_s = 0.02;
        config.settle_s = 0.01;
    }
    if let Some(value) = argument(args, "--points-step-mv") {
        config.step = value.parse::<f64>().map_err(|_| "Invalid step")? * 0.001;
    }
    iv_core::plan(&config)?;
    let client = client::Client::start(|| {})?;
    client.send(Command::Connect {
        simulation,
        source,
        meter,
    })?;
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        let state = client.snapshot();
        if state.connected {
            break;
        }
        if !state.error.is_empty() {
            return Err(state.error);
        }
        if Instant::now() > deadline {
            return Err("Connection timed out".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    client.send(Command::Run { config, path })?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let state = client.snapshot();
        if state.run_id > 0
            && matches!(
                state.phase.as_str(),
                "Completed" | "Stopped" | "Failed" | "Compliance stop"
            )
        {
            println!(
                "{}",
                serde_json::to_string(&state).map_err(|e| e.to_string())?
            );
            if state.phase != "Completed"
                || state.output != "Off"
                || !state.error.is_empty()
                || state.points != state.total
            {
                return Err(format!("Run did not complete cleanly: {}", state.message));
            }
            client.send(Command::Disconnect)?;
            return Ok(());
        }
        if !state.error.is_empty() && state.run_id == 0 {
            return Err(state.error);
        }
        if Instant::now() > deadline {
            client.send(Command::Stop)?;
            return Err("Measurement timed out; stop requested".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
