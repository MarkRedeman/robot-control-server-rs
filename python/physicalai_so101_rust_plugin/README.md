# PhysicalAI SO-101 Rust Plugin

A [PhysicalAI](https://github.com/openvinotoolkit/physicalai) plugin that binds
the Rust [`robot-control-server`](https://github.com/geti/robot-control-server)
library to Python, exposing a Rust-backed SO-101 robot (6× Feetech STS3215
servos) and a UVC camera. All hardware I/O, unit conversion, and the
connection/torque lifecycle live in Rust; the Python layer is a thin adapter
over the `Robot` and `Camera` interfaces.

## Repository layout

This package lives at the root of the `robot-control-server` repository:

```
robot-control-server/                     # ← repo root
├── Cargo.toml                            # Rust crate manifest
├── pyproject.toml                        # Python package manifest (maturin)
├── src/                                  # Rust source
└── python/
    └── physicalai_so101_rust_plugin/     # Python source (the package)
```

The examples below say "from the repo root"; that always means
`cd /path/to/robot-control-server` — the directory containing `pyproject.toml`
and `Cargo.toml`.

## Installation (from a release)

From your own project directory (the one that has the venv you want to install
into):

```bash
cd /path/to/your/project
uv add physicalai-so101-rust-plugin
```

or, using pip inside your project's venv:

```bash
pip install physicalai-so101-rust-plugin
```

Prebuilt wheels are published for Linux (x86_64 and aarch64). Installing
registers `SO101Rust_Follower` and `SO101Rust_Leader` with the Physical AI
Studio robot catalog.

## Editable install (for development)

An editable install builds the Rust extension once but lets you edit the Python
sources in place. Run it from the repo root:

```bash
cd /path/to/robot-control-server
uv pip install -e .            # editable install into the active venv
```

or with plain pip:

```bash
cd /path/to/robot-control-server
pip install -e .
```

Alternatively, `maturin develop` builds the Rust extension and installs the
package without requiring a separate `pip` step:

```bash
cd /path/to/robot-control-server
uvx maturin develop            # runs maturin without installing it globally
```

> If you are developing against a local `physicalai` checkout rather than the
> published release, install it first, e.g.
> `uv pip install -e /path/to/physicalai`, then install this package.

## Installing from a relative path

You can also point `uv`/`pip` at a relative path to this repository from your
own project directory. From your project directory:

```bash
cd /path/to/your/project
uv add --editable ../robot-control-server
```

or, with pip:

```bash
cd /path/to/your/project
pip install -e ../robot-control-server
```

In both cases the path (`../robot-control-server`) is resolved relative to the
current working directory, so adjust it to wherever this repository is checked
out.

## Robot usage

```python
from physicalai_so101_rust_plugin import SO101Rust

follower = SO101Rust(port="/dev/ttyUSB0", calibration="follower.json", unit="normalized")
follower.connect()
observation = follower.get_observation()          # joint_positions: np.ndarray (6,)
follower.send_action(observation.joint_positions) # teleop pass-through
follower.disconnect()
```

`unit` selects the joint-space representation:

| Unit         | Body joints                    | Gripper   | Calibration |
| ------------ | ------------------------------ | --------- | ----------- |
| `normalized` | `[-100, 100]` over calibrated range | `[0, 100]` | required |
| `degrees`    | degrees relative to range midpoint  | `[0, 100]` | required |
| `ticks`      | raw encoder `[0, 4095]`        | `[0, 4095]` | not allowed |

`calibration` accepts a lerobot calibration JSON mapping (joint name →
`{id, drive_mode, homing_offset, range_min, range_max}`) or a path to one.
`enable_torque_on_connect` (default `True`) enables torque after `connect()`;
leaders used for teleoperation should set it to `False`.

## Camera usage

```python
from physicalai_so101_rust_plugin import SO101RustUVCCamera

camera = SO101RustUVCCamera(device=0, width=640, height=480, fps=30)
camera.connect()
frame = camera.read()           # Frame with an (H, W, 3) uint8 RGB data array
camera.disconnect()
```

`device` accepts an integer index, a decimal string, or a `/dev/videoN` path.
Cameras are enumerated with `physicalai_so101_rust_plugin._native.discover_cameras()`.

## Studio integration

The plugin registers two catalog entries via the
`physicalai.studio.catalog_plugins` entry point:

- `SO101Rust_Follower` — torque enabled on connect, accepts actions.
- `SO101Rust_Leader` — torque off on connect, read-only for teleoperation.

## URDF assets

This package vendors the SO-101 follower/leader URDF descriptions and meshes
from [`TheRobotStudio/SO-ARM100`](https://github.com/TheRobotStudio/SO-ARM100)
(via the PhysicalAI SO-101 plugin):

```python
from physicalai_so101_rust_plugin import get_urdf_path

root = get_urdf_path()
follower = root / "so101-follower" / "so101_follower.urdf"
leader = root / "so101-leader" / "urdf" / "so101_leader.urdf"
```

## Building a wheel from source

From the repo root:

```bash
cd /path/to/robot-control-server
uvx maturin build --release --out dist
```

This compiles the Rust library (`robot-control-server`) as a `cdylib` behind the
`python` feature and packages it as the `physicalai_so101_rust_plugin._native`
extension module. The wheel is written to `dist/`.

## Development

Run the Python tests from the repo root (they mock the native module, so no
hardware or compiled extension is required):

```bash
cd /path/to/robot-control-server
uv run pytest
```

Run the Rust tests from the repo root:

```bash
cd /path/to/robot-control-server
cargo test --all-features
```
