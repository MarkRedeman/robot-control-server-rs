//! Unit-aware robot driver interface.
//!
//! This is the integration surface exposed to the Python bindings (and,
//! eventually, any other consumer that wants a PhysicalAI-shaped robot). It
//! deliberately keeps the connection/torque lifecycle and unit conversion in
//! Rust so the language binding stays a thin adapter.

use anyhow::Result;

/// A unit-converted observation from a robot.
#[derive(Debug, Clone)]
pub struct RobotObservation {
    /// Joint positions in the driver's configured unit, ordered to match
    /// [`RobotDriver::joint_names`].
    pub joint_positions: Vec<f64>,
}

/// Structural interface for a robot that can be observed and commanded in a
/// single configured [`JointUnit`].
pub trait RobotDriver: Send + Sync {
    /// Ordered joint names. Must match the position/action array ordering.
    fn joint_names(&self) -> &'static [&'static str];

    /// Canonical identity of the physical device this instance owns.
    ///
    /// Must be derivable from constructor parameters alone, with no hardware
    /// I/O.
    fn device_id(&self) -> String;

    /// Establish the connection to the hardware. Idempotent.
    fn connect(&self) -> Result<()>;

    /// Disconnect, leaving the robot in a safe state. Idempotent.
    fn disconnect(&self) -> Result<()>;

    /// Whether the robot is currently connected and ready for commands.
    fn is_connected(&self) -> bool;

    /// Read the current joint positions in the configured unit.
    fn get_observation(&self) -> Result<RobotObservation>;

    /// Command a target joint position (in the configured unit).
    fn send_action(&self, action: &[f64]) -> Result<()>;

    /// Enable or disable torque on all joints.
    fn set_torque(&self, enabled: bool) -> Result<()>;
}
