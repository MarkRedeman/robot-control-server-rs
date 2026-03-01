pub mod calibration;
pub mod client;
pub mod commands;
pub mod feetech;
pub mod serial;

pub use client::{ArmState, RobotClient};
pub use commands::{handle_command, parse_command, RobotCommand, RobotResponse};
pub use feetech::{load_calibration, FeetechRobotClient};
