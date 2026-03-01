pub mod calibration;
pub mod client;
pub mod commands;
pub mod feetech;
pub mod serial;
pub mod worker;

pub use client::{ArmState, RobotClient};
pub use commands::{RobotCommand, RobotResponse, handle_command, parse_command};
pub use feetech::{ArmCalibration, FeetechRobotClient, load_calibration};
pub use worker::{RobotWorkerConfig, RobotWorkerHandle, spawn_worker};
