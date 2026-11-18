# Copyright (C) 2026 Intel Corporation
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import numpy as np
from physicalai.robot import Robot

from physicalai_so101_rust_plugin.robot import SO101Rust, SO101RustObservation, _calibration_to_json


def test_is_a_robot_protocol_instance() -> None:
    robot = SO101Rust(port="/dev/ttyUSB0", unit="ticks")
    assert isinstance(robot, Robot)


def test_joint_names_and_device_ids() -> None:
    robot = SO101Rust(port="/dev/ttyUSB0", unit="ticks")
    assert robot.joint_names == [
        "shoulder_pan",
        "shoulder_lift",
        "elbow_flex",
        "wrist_flex",
        "wrist_roll",
        "gripper",
    ]
    assert robot.device_ids == ("serial:ttyUSB0",)


def test_calibration_to_json() -> None:
    assert _calibration_to_json(None) is None
    assert _calibration_to_json({"gripper": {"id": 6}}) == '{"gripper": {"id": 6}}'


def test_connect_disconnect_and_torque() -> None:
    robot = SO101Rust(port="/dev/ttyUSB0", unit="ticks", enable_torque_on_connect=True)
    robot.connect()
    assert robot.is_connected()
    assert robot._native.torque is True
    robot.disconnect()
    assert not robot.is_connected()


def test_get_observation() -> None:
    robot = SO101Rust(port="/dev/ttyUSB0", unit="ticks")
    observation = robot.get_observation()
    assert isinstance(observation, SO101RustObservation)
    np.testing.assert_array_equal(
        observation.joint_positions,
        np.array([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], dtype=np.float64),
    )
    assert isinstance(observation.timestamp, float)


def test_send_action_delegates_to_native() -> None:
    robot = SO101Rust(port="/dev/ttyUSB0", unit="ticks")
    action = np.array([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], dtype=np.float64)
    robot.send_action(action)
    np.testing.assert_array_equal(robot._native.sent_actions[0], action)
