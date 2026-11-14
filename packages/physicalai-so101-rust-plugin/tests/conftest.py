# Copyright (C) 2026 Intel Corporation
# SPDX-License-Identifier: Apache-2.0

"""Shared fixtures that replace the compiled Rust extension with a fake.

This lets the Python wrappers be unit-tested without building the native
module or attaching any hardware.
"""

from __future__ import annotations

import sys
import types

import numpy as np
import pytest


class FakeRustRobot:
    def __init__(self, port, baudrate, unit, calibration=None):
        self.port = port
        self.baudrate = baudrate
        self.unit = unit
        self.calibration = calibration
        self.connected = False
        self.torque = None
        self.sent_actions = []

    def connect(self) -> None:
        self.connected = True

    def disconnect(self) -> None:
        self.connected = False

    def is_connected(self) -> bool:
        return self.connected

    def device_id(self) -> str:
        return f"serial:{self.port.rsplit('/', 1)[-1]}"

    def joint_names(self) -> list[str]:
        return ["shoulder_pan", "shoulder_lift", "elbow_flex", "wrist_flex", "wrist_roll", "gripper"]

    def get_observation(self) -> np.ndarray:
        return np.array([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], dtype=np.float64)

    def send_action(self, action) -> None:
        self.sent_actions.append(np.asarray(action).copy())

    def set_torque(self, enabled: bool) -> None:
        self.torque = enabled


class FakeRustCamera:
    def __init__(self, index, width, height, fps):
        self.index = index
        self.width = width
        self.height = height
        self.fps = fps
        self.connected = False

    def connect(self) -> None:
        self.connected = True

    def disconnect(self) -> None:
        self.connected = False

    def is_connected(self) -> bool:
        return self.connected

    def read(self) -> np.ndarray:
        return np.zeros((self.height, self.width, 3), dtype=np.uint8)

    def read_latest(self) -> np.ndarray:
        return self.read()


@pytest.fixture(autouse=True)
def _fake_native(monkeypatch) -> None:
    """Install a fake `physicalai_so101_rust_plugin._native` module."""
    mod = types.ModuleType("physicalai_so101_rust_plugin._native")
    mod.RustRobot = FakeRustRobot
    mod.RustCamera = FakeRustCamera
    mod.discover_cameras = lambda: [("/dev/video0", 0, "Fake Cam")]
    mod.query_camera_formats = lambda index: [(640, 480, 30)]
    monkeypatch.setitem(sys.modules, "physicalai_so101_rust_plugin._native", mod)
