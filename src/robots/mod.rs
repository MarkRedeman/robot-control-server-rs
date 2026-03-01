pub mod calibration;
pub mod client;
pub mod commands;
pub mod feetech;
pub mod serial;
pub mod worker;

pub use client::{ArmState, RobotClient};
pub use commands::{handle_command, parse_command, RobotCommand, RobotResponse};
pub use feetech::{load_calibration, ArmCalibration, FeetechRobotClient};
pub use worker::{spawn_worker, RobotWorkerConfig, RobotWorkerHandle};
