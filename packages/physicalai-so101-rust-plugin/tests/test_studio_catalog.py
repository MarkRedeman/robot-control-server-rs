# Copyright (C) 2026 Intel Corporation
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import pytest

from physicalai_so101_rust_plugin.studio_catalog import (
    SO101RustLeaderPayload,
    SO101RustRobotPayload,
    register_physicalai_studio_plugin,
)


def test_payload_requires_identifier() -> None:
    with pytest.raises(ValueError):
        SO101RustRobotPayload()


def test_payload_accepts_serial_number() -> None:
    payload = SO101RustRobotPayload(serial_number="SO101-001")
    assert payload.serial_number == "SO101-001"


def test_leader_defaults_to_torque_off() -> None:
    assert SO101RustRobotPayload(serial_number="x").enable_torque_on_connect is True
    assert SO101RustLeaderPayload(serial_number="x").enable_torque_on_connect is False


def test_registers_follower_and_leader() -> None:
    class Registry:
        def __init__(self) -> None:
            self.types: list[str] = []

        def register_robot(self, definition) -> None:
            self.types.append(definition.type)

    registry = Registry()
    register_physicalai_studio_plugin(registry)
    assert registry.types == ["SO101Rust_Follower", "SO101Rust_Leader"]
