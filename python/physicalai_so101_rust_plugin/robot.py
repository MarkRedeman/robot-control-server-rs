# Copyright (C) 2026 Intel Corporation
# SPDX-License-Identifier: Apache-2.0

"""SO-101 robot driver backed by the Rust `robot-control-server` library.

The native extension owns all unit conversion, connection/torque lifecycle,
and observation/action shaping. This module is a thin type adapter onto the
:class:`~physicalai.robot.Robot` protocol.
"""

from __future__ import annotations

import json
import time
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

import numpy as np

from physicalai.config import export_config
from physicalai.robot import Robot

if TYPE_CHECKING:
    from physicalai.capture.frame import Frame

JOINT_ORDER: tuple[str, ...] = (
    "shoulder_pan",
    "shoulder_lift",
    "elbow_flex",
    "wrist_flex",
    "wrist_roll",
    "gripper",
)


def _calibration_to_json(calibration: dict | str | Path | None) -> str | None:
    """Normalize a lerobot calibration into a JSON string for the native layer."""
    if calibration is None:
        return None
    if isinstance(calibration, (str, Path)):
        calibration = json.loads(Path(calibration).read_text(encoding="utf-8"))
    return json.dumps(calibration)


@dataclass
class SO101RustObservation:
    """Observation from the Rust-backed SO-101 robot arm."""

    joint_positions: np.ndarray
    timestamp: float
    sensor_data: dict[str, np.ndarray] | None = None
    images: dict[str, "Frame"] | None = None

    @property
    def state(self) -> np.ndarray:
        """State vector: joint positions ``(6,)``."""
        return self.joint_positions


@export_config(class_path="physicalai_so101_rust_plugin.SO101Rust")
class SO101Rust(Robot):
    """Driver for the SO-101 arm backed by Rust.

    Args:
        port: Serial port path, e.g. ``"/dev/ttyUSB0"``.
        calibration: Lerobot calibration mapping (joint name → calibration dict)
            or a path to a calibration JSON file. Required for ``normalized``
            and ``degrees`` units.
        baudrate: Serial baudrate (default ``1_000_000``).
        unit: Joint-space unit — ``"normalized"``, ``"degrees"``, or ``"ticks"``.
        enable_torque_on_connect: Enable torque after connecting. Followers
            should default to ``True``; leaders (read-only teleop) to ``False``.
    """

    JOINT_ORDER: tuple[str, ...] = JOINT_ORDER

    def __init__(
        self,
        port: str,
        calibration: dict | str | Path | None = None,
        baudrate: int = 1_000_000,
        unit: str = "normalized",
        *,
        enable_torque_on_connect: bool = True,
    ) -> None:
        from physicalai_so101_rust_plugin import _native

        self._port = port
        self._baudrate = baudrate
        self._unit = unit
        self._enable_torque_on_connect = enable_torque_on_connect
        self._native = _native.RustRobot(
            port,
            baudrate,
            unit,
            _calibration_to_json(calibration),
        )

    @property
    def port(self) -> str:
        """Serial port path."""
        return self._port

    @property
    def unit(self) -> str:
        """Configured joint-space unit."""
        return self._unit

    @property
    def joint_names(self) -> list[str]:
        """Ordered joint names."""
        return list(self.JOINT_ORDER)

    @property
    def device_ids(self) -> tuple[str, ...]:
        """Canonical physical-device identity, derived with no hardware I/O."""
        return (self._native.device_id(),)

    def connect(self) -> None:
        """Open the serial port and apply the configured torque state."""
        self._native.connect()
        self._native.set_torque(self._enable_torque_on_connect)

    def disconnect(self) -> None:
        """Disconnect, leaving the robot with torque off."""
        self._native.disconnect()

    def is_connected(self) -> bool:
        """Whether the robot is currently connected."""
        return self._native.is_connected()

    def get_observation(self) -> SO101RustObservation:
        """Read the current joint positions in the configured unit."""
        positions = self._native.get_observation()
        return SO101RustObservation(
            joint_positions=positions,
            timestamp=time.monotonic(),
        )

    def send_action(self, action: np.ndarray, *, goal_time: float = 0.1) -> None:
        """Send a joint command in the configured unit."""
        _ = goal_time
        self._native.send_action(action)

    def set_torque(self, *, enabled: bool) -> None:
        """Enable or disable torque on all joints."""
        self._native.set_torque(enabled)
