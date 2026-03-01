# robot-control-server

A Rust server for controlling SO101 robotic arms (6x Feetech STS3215 servos each) via REST API and WebSocket, built on [Poem](https://poem.rs/) and [rustypot](https://github.com/pollen-robotics/rustypot).

## Features

- **REST API** — Read robot state, get robot details, send commands
- **WebSocket streaming** — Real-time robot state updates via WebSocket
- **Multi-robot support** — Manage multiple robots simultaneously
- **Swagger UI** — Interactive API documentation at `/swagger-ui`
- **Calibration support** — Load lerobot calibration JSON files for calibrated joint angles
- **Threaded architecture** — Each robot is read on its own thread for maximum throughput

## Usage

### Running the Server

```bash
# Default (host: 0.0.0.0, port: 8000)
cargo run --bin server

# With custom settings (via environment or config)
ROBOT_CONTROL_HOST=127.0.0.1 ROBOT_CONTROL_PORT=9000 cargo run --bin server
```

### API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/ready` | GET | Readiness check with connected robot count |
| `/config` | GET | Server configuration |
| `/debug/ports` | GET | List available serial ports |
| `/api/robots` | GET | List all available robots |
| `/api/robots/:serial_id` | GET | Get robot details (port, connection status) |
| `/api/robots/:serial_id/state` | GET | Get current robot state |
| `/api/robots/:serial_id/state?normalize=true` | GET | Get normalized robot state |
| `/api/robots/:serial_id/command` | POST | Send command to robot |
| `/api/robots/:serial_id/ws` | WS | WebSocket for real-time state updates |
| `/api/cameras` | GET | List available cameras |
| `/api/cameras/supported_formats/:driver/:fingerprint` | GET | Get supported formats for a camera |
| `/api/cameras/ws` | WS | WebSocket for camera streaming |

### WebSocket

Connect to `/api/robots/:serial_id/ws` to receive real-time state updates:

```bash
# Example: connect with wscat
wscat -c ws://localhost:8000/api/robots/YOUR_SERIAL_ID/ws?fps=30
```

Query parameters:
- `fps` — Frames per second (default: 30)

### Camera API

List available cameras:

```bash
curl http://localhost:8000/api/cameras
```

Response:
```json
[
  {
    "driver": "usb_camera",
    "fingerprint": "0",
    "hardware_name": "Logitech C920",
    "default_profile": {"width": 640, "height": 480, "fps": [30]}
  }
]
```

Get supported formats for a camera:

```bash
curl http://localhost:8000/api/cameras/supported_formats/usb_camera/0
```

Response:
```json
[
  {"width": 1920, "height": 1080, "fps": [30, 15, 5]},
  {"width": 1280, "height": 720, "fps": [60, 30, 15]},
  {"width": 640, "height": 480, "fps": [120, 60, 30, 15]}
]
```

### Camera WebSocket

Connect to `/api/cameras/ws` to stream camera frames:

```bash
# Example: connect to first camera at 640x480
wscat -c 'ws://localhost:8000/api/cameras/ws?camera={"driver":"usb_camera","fingerprint":"0","width":640,"height":480,"fps":30}'
```

Query parameter:
- `camera` — JSON-encoded camera configuration with `driver`, `fingerprint`, `width`, `height`, `fps`

The server sends binary JPEG frames. Status messages are sent as JSON:
```json
{"event": "status", "state": "running", "message": "..."}
```

Client can send:
- `{"command": "ping"}` — Keep-alive ping
- `{"command": "disconnect"}` — Request graceful disconnect

### Examples

```bash
# List available robots
curl http://localhost:8000/api/robots

# Get robot state
curl http://localhost:8000/api/robots/YOUR_SERIAL_ID/state

# Send command (set joint positions)
curl -X POST http://localhost:8000/api/robots/YOUR_SERIAL_ID/command \
  -H "Content-Type: application/json" \
  -d '{"joints": {"shoulder_pan": 0, "shoulder_lift": -45, "elbow_flex": 90}}'

# Open Swagger UI
# Visit http://localhost:8000/swagger-ui
```

## Building

### Standard Build
```bash
cargo build --release
```

The binary will be at `target/release/server`.

### Optimized Build
For the best runtime performance and smallest binary size, you can build an optimized release by enabling LTO (Link Time Optimization), aborting on panic, and stripping symbols. You can run the following to build it with these flags via environment variables without changing the `Cargo.toml`:

```bash
CARGO_PROFILE_RELEASE_LTO=true \
CARGO_PROFILE_RELEASE_PANIC=abort \
CARGO_PROFILE_RELEASE_STRIP=symbols \
CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
cargo build --release
```

### Permissions

The serial port typically requires membership in the `dialout` group:

```bash
sudo usermod -aG dialout $USER
# Log out and back in for the change to take effect
```

## Architecture

- **RobotClient trait** — Abstract interface for robot implementations
- **FeetechRobotClient** — Concrete implementation for Feetech STS3215 servos
- **AppState** — Manages robot instances and their snapshots
- **Poem/OpenAPI** — REST API with automatic Swagger documentation

## CLI Tool

A CLI tool is also provided in this crate for direct serial port access:

```bash
# One-shot read
cargo run --bin cli -- --port=/dev/ttyUSB0

# Continuous monitoring
cargo run --bin cli -- --port=/dev/ttyUSB0 --watch

# Monitor using serial numbers and calibration files
cargo run --bin cli -- --serial-number=5AAF270379 --calibration=calibration/follower.json \
    --serial-number=5AAF270363 --calibration=calibration/leader.json \
    --watch --interval=0
```

### CLI Options

| Flag | Description | Default |
|---|---|---|
| `--serial-number <SN>` | Find serial port by USB device serial number (repeatable) | - |
| `--port <PATH>` | Explicit serial port path (repeatable) | - |
| `--baudrate <RATE>` | Baud rate for serial communication | `1000000` |
| `-w`, `--watch` | Continuously monitor arm state | off |
| `--interval <MS>` | Polling interval in milliseconds | `16` |
| `--calibration <PATH>` | Path to lerobot calibration JSON file (repeatable) | - |
