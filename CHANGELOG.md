# Changelog

## 0.1.0

Initial release of the Rust-backed PhysicalAI SO-101 plugin:

- `SO101Rust` — a `physicalai.robot.Robot` implementation backed by the Rust
  `robot-control-server` library (normalized/degrees/ticks units).
- `SO101RustUVCCamera` — a `physicalai.capture.Camera` implementation backed by
  the Rust nokhwa driver.
- Studio catalog registration for `SO101Rust_Follower` and `SO101Rust_Leader`,
  with bundled SO-101 URDF assets.
