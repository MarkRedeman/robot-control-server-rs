# Copyright (C) 2026 Intel Corporation
# SPDX-License-Identifier: Apache-2.0

"""Tests for the Rust-backed SO-101 visual identification routine."""

from __future__ import annotations

from types import SimpleNamespace
from typing import TYPE_CHECKING

import numpy as np
import physicalai_so101_rust_plugin.studio_catalog as studio_catalog
import pytest

if TYPE_CHECKING:
    from collections.abc import Generator

JOINT_NAMES = ("shoulder_pan", "shoulder_lift", "elbow_flex", "wrist_flex", "wrist_roll", "gripper")
PORT = "/dev/ttyACM0"


class _FakeDriver:
    """Stands in for the Rust-backed SO101Rust driver."""

    JOINT_ORDER = list(JOINT_NAMES)
    instances: list[_FakeDriver] = []
    default_start: np.ndarray = np.zeros(6, dtype=np.float32)
    default_observation_failure: Exception | None = None

    def __init__(self, *, port: str | None = None, unit: str | None = None) -> None:
        self.port = port
        self.unit = unit
        self.current_position = _FakeDriver.default_start.copy()
        self._observation_failure = _FakeDriver.default_observation_failure
        self.actions: list[np.ndarray] = []
        self.torque_commands: list[bool] = []
        self.connected = False
        self.disconnected = False
        _FakeDriver.instances.append(self)

    @classmethod
    def uncalibrated(
        cls,
        port: str,
        baudrate: int = 1_000_000,
        unit: str = "ticks",
        *,
        enable_torque_on_connect: bool = True,
    ) -> _FakeDriver:
        return cls(port=port, unit=unit)

    def connect(self) -> None:
        self.connected = True

    def get_observation(self) -> SimpleNamespace:
        if self._observation_failure is not None:
            raise self._observation_failure
        return SimpleNamespace(joint_positions=self.current_position.copy())

    def send_action(self, action: np.ndarray) -> None:
        self.actions.append(action.copy())
        self.current_position = action.copy()

    def set_torque(self, *, enabled: bool) -> None:
        self.torque_commands.append(enabled)

    def disconnect(self) -> None:
        self.disconnected = True


@pytest.fixture(autouse=True)
def _reset(monkeypatch: pytest.MonkeyPatch) -> Generator[None]:
    monkeypatch.setattr(studio_catalog, "SO101Rust", _FakeDriver)
    _FakeDriver.instances.clear()
    _FakeDriver.default_start = np.zeros(6, dtype=np.float32)
    _FakeDriver.default_observation_failure = None
    yield
    _FakeDriver.instances.clear()


def _last_driver() -> _FakeDriver:
    return _FakeDriver.instances[-1]


def test_identify_wiggles_gripper_in_ticks_mode() -> None:
    start = np.zeros(6, dtype=np.float32)
    start[5] = 2048.0
    _FakeDriver.default_start = start

    studio_catalog.identify_so101_rust_visually(PORT)

    driver = _last_driver()
    assert driver.unit == "ticks"
    assert driver.connected
    assert driver.disconnected
    assert driver.torque_commands == [False]
    assert len(driver.actions) == 3

    gripper_goals = [float(action[5]) for action in driver.actions]
    assert all(0.0 <= goal <= 4095.0 for goal in gripper_goals)
    assert gripper_goals[-1] == 2048.0


def test_identify_reports_overload_protection() -> None:
    _FakeDriver.default_start = np.array([0, 0, 0, 0, 0, 2048.0], dtype=np.float32)
    _FakeDriver.default_observation_failure = RuntimeError("Servo 'gripper' (ID 6) did not respond")

    with pytest.raises(RuntimeError, match="overload protection"):
        studio_catalog.identify_so101_rust_visually(PORT)

    driver = _last_driver()
    assert driver.disconnected
    assert driver.torque_commands == [False]


def test_identify_lets_connect_errors_propagate(monkeypatch: pytest.MonkeyPatch) -> None:
    class _FailingDriver(_FakeDriver):
        def connect(self) -> None:
            raise RuntimeError(f"Failed to open serial port {PORT}")

    monkeypatch.setattr(studio_catalog, "SO101Rust", _FailingDriver)

    with pytest.raises(RuntimeError, match="Failed to open serial port"):
        studio_catalog.identify_so101_rust_visually(PORT)

    assert not _last_driver().disconnected
    assert _last_driver().torque_commands == []
