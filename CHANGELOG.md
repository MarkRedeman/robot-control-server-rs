# Changelog

## [0.2.0](https://github.com/MarkRedeman/robot-control-server-rs/compare/v0.1.0...v0.2.0) (2026-11-18)


### ✨ Features

* **cameras:** add synchronous nokhwa camera driver for Python bindings ([c12885a](https://github.com/MarkRedeman/robot-control-server-rs/commit/c12885a9d06f309ca7a5d6b9d68bb2bc09c5b7a6))
* **python:** add PhysicalAI Robot and Camera wrappers and Studio catalog plugin ([283bad2](https://github.com/MarkRedeman/robot-control-server-rs/commit/283bad279f167446f4489e6870a93a7f606a9d82))
* **python:** add PyO3 bindings for robot and camera drivers ([205c9e9](https://github.com/MarkRedeman/robot-control-server-rs/commit/205c9e90dcd30e17836b5471ddd5f908ddd1faff))
* **python:** vendor SO-101 URDF assets and wire Studio catalog asset ([8c92761](https://github.com/MarkRedeman/robot-control-server-rs/commit/8c927618bd15a03744bbcd949ba9ceb439b68ad6))
* **robots:** add unit-aware RobotDriver and lazy-connect FeetechRobotClient ([da1f31e](https://github.com/MarkRedeman/robot-control-server-rs/commit/da1f31e5845b876cbcdbdf5992c124c18af306db))


### 🐛 Bug Fixes

* **cli:** satisfy clippy manual_checked_ops lint ([c4a9eee](https://github.com/MarkRedeman/robot-control-server-rs/commit/c4a9eee0318c13b883d1d36ec8ac9ad85bf60448))
* **python:** map identify errors to overload protection and add uncalibrated factory ([e2e1773](https://github.com/MarkRedeman/robot-control-server-rs/commit/e2e177391438539ba697629070c6d783720613f4))
* **robots:** ping servos on connect to verify hardware responds ([84910d1](https://github.com/MarkRedeman/robot-control-server-rs/commit/84910d1cc88cbb1e56ec964c2d5cbbf05f1d3e04))


### ♻️ Code Refactoring

* **python:** derive device_ids from the native driver ([bf8bde4](https://github.com/MarkRedeman/robot-control-server-rs/commit/bf8bde46d204894e67973fa4e002920ef536a592))
* **python:** restructure to packages/&lt;name&gt;/src layout and use uv ([a325d0c](https://github.com/MarkRedeman/robot-control-server-rs/commit/a325d0c7c3e57324fc7f84250a8328f66e3b9bb9))


### 📚 Documentation

* add PhysicalAI package integration plan ([6a53506](https://github.com/MarkRedeman/robot-control-server-rs/commit/6a5350674f891e1991fde5cd1a0ad75e950a8943))
* document the Physical AI Studio plugin ([7b2d3e9](https://github.com/MarkRedeman/robot-control-server-rs/commit/7b2d3e94423bab792cc6f993504aaf7bac8c40e6))
* **python:** add package README and finalize project metadata ([7baeca2](https://github.com/MarkRedeman/robot-control-server-rs/commit/7baeca268b7c06966143e2fc0a76f12dd553cb28))
* remove plan ([c2cebe4](https://github.com/MarkRedeman/robot-control-server-rs/commit/c2cebe439cf8c1df90e7ca8f1612a93816a45a72))


### ✅ Tests

* **python:** cover Robot/Camera wrappers and Studio catalog registration ([3193351](https://github.com/MarkRedeman/robot-control-server-rs/commit/31933512c97d44faa9861dce475b87325f81ac76))
* **robots:** cover unit conversion against lerobot fixture ([2d404af](https://github.com/MarkRedeman/robot-control-server-rs/commit/2d404afaaf23eee7f886d43215d8406ee380c8d9))


### 👷 CI/CD

* add Python CI/CD and release-please configuration ([8d70d3b](https://github.com/MarkRedeman/robot-control-server-rs/commit/8d70d3b847bb7b954ef5054278915b9cfbca8c11))
* fetch Git LFS assets in Python workflows ([8089c18](https://github.com/MarkRedeman/robot-control-server-rs/commit/8089c18b332fdaec163807c58a8e16aa6ba3bb8d))


### 🔧 Chores

* update Cargo.lock for python feature dependencies ([93a207e](https://github.com/MarkRedeman/robot-control-server-rs/commit/93a207ebfd5bb0987e8f3b56e25f6a9dbed324d3))

## 0.1.0

Initial release of the Rust-backed PhysicalAI SO-101 plugin:

- `SO101Rust` — a `physicalai.robot.Robot` implementation backed by the Rust
  `robot-control-server` library (normalized/degrees/ticks units).
- `SO101RustUVCCamera` — a `physicalai.capture.Camera` implementation backed by
  the Rust nokhwa driver.
- Studio catalog registration for `SO101Rust_Follower` and `SO101Rust_Leader`,
  with bundled SO-101 URDF assets.
