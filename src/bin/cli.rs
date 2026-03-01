use anyhow::{bail, Context, Result};
use clap::Parser;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use robot_control_server::cli::display;
use robot_control_server::robots::feetech;
use robot_control_server::robots::feetech::ArmCalibration;
use robot_control_server::robots::serial;
use robot_control_server::robots::{
    spawn_worker, ArmState, FeetechRobotClient, RobotCommand, RobotResponse, RobotWorkerConfig,
    RobotWorkerHandle,
};

/// Read the state of one or more SO101 robotic arms (6x Feetech STS3215 servos each).
#[derive(Parser, Debug)]
#[command(name = "robot-control", version, about)]
struct Cli {
    /// USB device serial number(s) to find serial ports automatically (repeatable).
    #[arg(long)]
    serial_number: Vec<String>,

    /// Explicit serial port path(s) (e.g. /dev/ttyUSB0) (repeatable).
    #[arg(long)]
    port: Vec<String>,

    /// Baud rate for serial communication.
    #[arg(long, default_value_t = 1_000_000)]
    baudrate: u32,

    /// Continuously monitor the arm state.
    #[arg(short, long)]
    watch: bool,

    /// Polling interval in milliseconds (used with --watch). Default targets ~60 fps.
    #[arg(long, default_value_t = 16)]
    interval: u64,

    /// Path to a lerobot calibration JSON file (repeatable, one per robot in the
    /// same order as --serial-number / --port).
    #[arg(long)]
    calibration: Vec<PathBuf>,
}

fn resolve_ports(cli: &Cli) -> Result<Vec<String>> {
    if cli.serial_number.is_empty() && cli.port.is_empty() {
        bail!(
            "At least one --serial-number or --port must be specified.\n\
             Use --serial-number=<SN> to auto-detect, or --port=<path> for an explicit path.\n\
             Both flags can be repeated for multiple robots."
        );
    }

    let mut ports = Vec::new();

    for sn in &cli.serial_number {
        ports.push(serial::find_port_by_serial_number(sn)?);
    }
    for p in &cli.port {
        ports.push(p.clone());
    }

    Ok(ports)
}

/// Load calibration files and validate the count matches the number of robots.
/// Returns a Vec of Option<ArmCalibration> maps, one per robot.
fn load_calibrations(paths: &[PathBuf], num_robots: usize) -> Result<Vec<Option<ArmCalibration>>> {
    if paths.is_empty() {
        return Ok(vec![None; num_robots]);
    }

    if paths.len() != num_robots {
        bail!(
            "Number of --calibration files ({}) must match the number of robots ({}).\n\
             Provide one calibration file per robot in the same order as --serial-number / --port.",
            paths.len(),
            num_robots,
        );
    }

    let mut result = Vec::with_capacity(num_robots);
    for path in paths {
        let arm_cal = feetech::load_calibration(path)?;
        result.push(Some(arm_cal));
    }
    Ok(result)
}

/// A named robot worker.
struct Robot {
    label: String,
    handle: RobotWorkerHandle,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let port_paths = resolve_ports(&cli)?;

    // Load calibration files (if any). Each calibration file corresponds
    // positionally to a robot port.
    let calibrations = load_calibrations(&cli.calibration, port_paths.len())?;

    // Convert --interval to fps for the worker config.
    let fps = if cli.interval > 0 {
        (1000 / cli.interval).clamp(1, 240) as u32
    } else {
        240 // max throughput
    };

    let mut robots = Vec::with_capacity(port_paths.len());
    for (idx, port_path) in port_paths.iter().enumerate() {
        eprintln!(
            "Connecting to serial port: {port_path} at {} baud",
            cli.baudrate
        );

        let calibration = calibrations.get(idx).cloned().flatten();

        let client = Arc::new(
            FeetechRobotClient::new(
                port_path.clone(),
                port_path.clone(),
                cli.baudrate,
                calibration,
            )
            .with_context(|| {
                format!(
                    "Failed to open serial port '{port_path}'.\n\
                     Hint: check permissions (you may need to add your user to the 'dialout' group \
                     or run with sudo)."
                )
            })?,
        );

        let handle = spawn_worker(client, RobotWorkerConfig { fps });

        robots.push(Robot {
            label: port_path.clone(),
            handle,
        });
    }

    if cli.watch {
        run_watch_mode(robots, cli.interval)
    } else {
        run_oneshot(&robots)
    }
}

fn run_oneshot(robots: &[Robot]) -> Result<()> {
    for robot in robots {
        let response = robot
            .handle
            .send_command_blocking(RobotCommand::ReadState)
            .map_err(|e| anyhow::anyhow!(e))?;

        match response {
            RobotResponse::State { state } => {
                println!("{}", display::format_arm_state(&robot.label, &state));
            }
            RobotResponse::Error { error, message } => {
                bail!(
                    "read_state failed for {}: {}: {}",
                    robot.label,
                    error,
                    message
                );
            }
            other => {
                bail!(
                    "Unexpected response from worker for {}: {:?}",
                    robot.label,
                    other
                );
            }
        }
    }
    Ok(())
}

fn run_watch_mode(mut robots: Vec<Robot>, interval_ms: u64) -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .context("Failed to set Ctrl-C handler")?;

    let interval = Duration::from_millis(interval_ms);
    let num_robots = robots.len();
    eprintln!("Watching arm state (press Ctrl-C to stop)...\n");

    // Take the state receivers from each worker handle.
    let mut state_receivers: Vec<(String, tokio::sync::mpsc::Receiver<ArmState>)> = Vec::new();
    let mut latest_states: Vec<Option<ArmState>> = Vec::new();

    for robot in &mut robots {
        let rx = robot
            .handle
            .take_state_rx()
            .expect("state_rx should be available");
        state_receivers.push((robot.label.clone(), rx));
        latest_states.push(None);
    }

    // Display loop on the main thread.
    let mut last_tick = Instant::now();
    let mut avg_fps: f64 = 0.0;

    while running.load(Ordering::SeqCst) {
        let loop_start = Instant::now();

        let dt = loop_start.duration_since(last_tick).as_secs_f64();
        if dt > 0.0 {
            let instant_fps = 1.0 / dt;
            avg_fps = if avg_fps == 0.0 {
                instant_fps
            } else {
                avg_fps * 0.9 + instant_fps * 0.1
            };
        }
        last_tick = loop_start;

        // Drain all available state updates from each receiver.
        for (i, (_label, rx)) in state_receivers.iter_mut().enumerate() {
            // Keep draining — we only care about the latest state.
            while let Ok(state) = rx.try_recv() {
                latest_states[i] = Some(state);
            }
        }

        // Move cursor to top-left without clearing — content is overwritten
        // in-place to avoid the full-screen flash that causes flicker.
        print!("\x1B[H");

        let mut had_success = false;
        for (i, (label, _rx)) in state_receivers.iter().enumerate() {
            if let Some(ref state) = latest_states[i] {
                println!("{}", display::format_arm_state(label, state));
                had_success = true;
            }
        }

        if had_success {
            println!(
                "Reading {} robot{} at {avg_fps:.1} fps",
                num_robots,
                if num_robots == 1 { "" } else { "s" },
            );
        }

        // Clear any leftover lines from a previous longer frame.
        print!("\x1B[J");

        // Sleep only for the remaining time in the interval, so the effective
        // loop period stays close to `interval_ms` regardless of read duration.
        // With --interval=0 no sleep occurs (max throughput for stress testing).
        let elapsed = loop_start.elapsed();
        if let Some(remaining) = interval.checked_sub(elapsed) {
            std::thread::sleep(remaining);
        }
    }

    // Request workers to stop.
    for robot in &robots {
        robot.handle.request_stop();
    }

    eprintln!("\nStopped.");
    Ok(())
}
