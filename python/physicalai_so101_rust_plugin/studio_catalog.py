# Copyright (C) 2026 Intel Corporation
# SPDX-License-Identifier: Apache-2.0

"""Studio catalog plugin for the Rust-backed SO-101 robot."""

from __future__ import annotations

import asyncio
import time
from typing import TYPE_CHECKING, Literal, Protocol, cast

from loguru import logger
from physicalai_studio_plugin import (
    CatalogRobot,
    CatalogRobotFactory,
    PortScanner,
    RobotCatalogDefinition,
    RobotProbe,
    SerialPortInfo,
    robot_field_ui,
    robot_payload_ui,
)
from pydantic import BaseModel, ConfigDict, Field, JsonValue, model_validator
from serial.tools import list_ports

from physicalai_so101_rust_plugin import SO101Rust

if TYPE_CHECKING:
    from collections.abc import Sequence


class _RobotCatalogRegistry(Protocol):
    def register_robot(self, definition: RobotCatalogDefinition) -> None: ...


class SO101RustRobotPayload(BaseModel):
    """Connection configuration for the Rust-backed SO-101 serial robot."""

    connection_string: str = Field(
        default="",
        description="Serial port path; leave empty to auto-discover via serial_number",
    )
    serial_number: str = Field(
        default="",
        description="USB serial number of the robot (when available)",
    )
    unit: Literal["normalized", "degrees", "ticks"] = Field(
        default="normalized",
        title="Joint units",
        description=(
            "Normalized maps body joints to [-100, 100]. Degrees reports body angles relative "
            "to the calibrated range midpoint. The gripper uses [0, 100] in both modes. Ticks "
            "uses raw servo encoder values and requires no calibration."
        ),
        json_schema_extra=cast(
            "dict[str, JsonValue]",
            robot_field_ui({
                "advanced_configuration": True,
                "info": {
                    "title": "Choosing joint units",
                    "description": (
                        "Normalized (default) and degrees require calibration. Ticks is raw "
                        "encoder mode for uncalibrated bringup. Choose matching units for leader "
                        "and follower arms and for your trained policy."
                    ),
                    "variant": "help",
                },
            }),
        ),
    )
    calibration: dict[str, dict[str, int]] | None = Field(
        default=None,
        description=(
            "Lerobot calibration mapping (joint name → {id, drive_mode, homing_offset, "
            "range_min, range_max}). Required for normalized and degrees units."
        ),
        json_schema_extra=cast(
            "dict[str, JsonValue]",
            robot_field_ui({
                "advanced_configuration": True,
                "info": {
                    "title": "Calibration values",
                    "description": (
                        "Upload a calibration JSON exported for this SO101. If provided, the "
                        "driver uses these values as-is."
                    ),
                    "variant": "help",
                },
            }),
        ),
    )
    enable_torque_on_connect: bool = Field(
        default=True,
        title="Enable torque on connect",
        description=(
            "Enable torque when connecting. Followers default to on; leaders default to off."
        ),
        json_schema_extra=cast(
            "dict[str, JsonValue]",
            robot_field_ui({"advanced_configuration": True}),
        ),
    )

    model_config = ConfigDict(
        json_schema_extra=cast(
            "dict[str, JsonValue]",
            {
                "example": {
                    "connection_string": "",
                    "serial_number": "SO101-2024-001",
                    "calibration": None,
                    "unit": "normalized",
                },
                **robot_payload_ui([
                    {
                        "kind": "connection",
                        "label": "Connection",
                        "device_discovery": True,
                        "identify": True,
                        "manual_entry": True,
                        "bind": {"connection": "connection_string", "serial_number": "serial_number"},
                    },
                    {"kind": "field", "name": "unit"},
                    {"kind": "field", "name": "enable_torque_on_connect"},
                    {"kind": "calibration", "name": "calibration"},
                ]),
            },
        ),
    )

    @model_validator(mode="after")
    def validate_identifier(self) -> SO101RustRobotPayload:
        """Require a serial number or an explicit serial-device path."""
        if not self.connection_string and not self.serial_number:
            msg = "Either serial_number or connection_string is required for SO101 Rust robots"
            raise ValueError(msg)
        return self


class SO101RustLeaderPayload(SO101RustRobotPayload):
    """Leader configuration with torque-off lifecycle defaults."""

    enable_torque_on_connect: bool = Field(
        default=False,
        title="Enable torque on connect",
        description="Enable torque when connecting. Leader arms remain read-only for teleoperation.",
        json_schema_extra=cast(
            "dict[str, JsonValue]",
            robot_field_ui({"advanced_configuration": True}),
        ),
    )


async def _build_so101_rust_driver(
    robot: CatalogRobot[SO101RustRobotPayload],
    factory: CatalogRobotFactory,
) -> SO101Rust:
    payload = robot.payload
    if not isinstance(payload, SO101RustRobotPayload):
        msg = f"Expected SO101RustRobotPayload, got {type(payload).__name__}"
        raise TypeError(msg)

    port = await factory.find_port(
        SerialPortInfo(
            connection_string=payload.connection_string or None,
            serial_number=payload.serial_number or None,
        )
    )
    if port is None:
        resource_key = payload.serial_number or payload.connection_string
        msg = f"Could not resolve a serial port for {resource_key}"
        raise ValueError(msg)

    if payload.calibration is None:
        if payload.unit != "ticks":
            logger.warning(
                "SO101 Rust has no calibration; building in raw-ticks mode. "
                "Positions are not normalized and must not be used for policy inference.",
            )
        return SO101Rust(
            port=port,
            calibration=None,
            unit="ticks",
            enable_torque_on_connect=payload.enable_torque_on_connect,
        )

    return SO101Rust(
        port=port,
        calibration=payload.calibration,
        unit=payload.unit,
        enable_torque_on_connect=payload.enable_torque_on_connect,
    )


def _resolve_serial_port(discovered: Sequence[SerialPortInfo], target: SerialPortInfo) -> str | None:
    if target.serial_number is not None:
        for serial_port in discovered:
            if serial_port.serial_number == target.serial_number:
                return serial_port.connection_string
        return None

    for serial_port in discovered:
        if serial_port.connection_string == target.connection_string:
            return serial_port.connection_string
    return None


async def find_so101_rust_port(manager: PortScanner, serial_port: SerialPortInfo) -> str | None:
    """Find the current port for a Rust SO-101 by serial number or configured port."""
    port = _resolve_serial_port(manager.robots, serial_port)
    if port is not None:
        return port
    await manager.find_robots()
    return _resolve_serial_port(manager.robots, serial_port)


_IDENTIFY_WIGGLE = 0.08


def identify_so101_rust_visually(connection_string: str) -> None:
    """Move the gripper a small way and back to identify a connected robot."""
    joint_index = SO101Rust.JOINT_ORDER.index("gripper")
    driver = SO101Rust(port=connection_string, unit="ticks", enable_torque_on_connect=True)
    driver.connect()
    try:
        observation = driver.get_observation()
        current = float(observation.joint_positions[joint_index])
        step = 4096 * _IDENTIFY_WIGGLE
        lo, hi = 0.0, 4095.0
        targets = (min(max(current + step, lo), hi), min(max(current - step, lo), hi), current)
        for target in targets:
            action = observation.joint_positions.copy()
            action[joint_index] = target
            driver.send_action(action)
            time.sleep(1.0)
            observation = driver.get_observation()
    finally:
        driver.set_torque(enabled=False)
        driver.disconnect()


class SO101RustProbe(RobotProbe[SO101RustRobotPayload]):
    """Probe for Rust-backed SO-101 robots — serial discovery and identification."""

    @staticmethod
    async def discover(manager: PortScanner) -> list[SerialPortInfo]:
        """Refresh and return the serial ports reported by the scanner."""
        await manager.find_robots()
        return manager.robots

    @staticmethod
    async def identify(
        payload: SO101RustRobotPayload,
        manager: PortScanner | None,
        joint: str | None = None,
    ) -> None:
        """Visually identify the configured robot by moving its gripper."""
        _ = joint
        if manager is None:
            msg = "PortScanner required for SO101 Rust identification"
            raise ValueError(msg)
        connection_string = await find_so101_rust_port(
            manager,
            SerialPortInfo(
                connection_string=payload.connection_string or None,
                serial_number=payload.serial_number or None,
            ),
        )
        if connection_string is None:
            if payload.serial_number:
                msg = f"Could not find the serial port for serial number {payload.serial_number}"
                raise ValueError(msg)
            msg = "Could not resolve a serial port from connection_string"
            raise ValueError(msg)
        await asyncio.to_thread(identify_so101_rust_visually, connection_string)

    @staticmethod
    async def is_online(payload: SO101RustRobotPayload, manager: PortScanner | None = None) -> bool:
        """Return whether the configured robot is currently discoverable."""
        serial_port = SerialPortInfo(
            connection_string=payload.connection_string or None,
            serial_number=payload.serial_number or None,
        )
        if manager is not None:
            return await find_so101_rust_port(manager, serial_port) is not None

        discovered = [
            SerialPortInfo(
                connection_string=port.device,
                serial_number=getattr(port, "serial_number", None) or None,
            )
            for port in list_ports.comports()
        ]
        return _resolve_serial_port(discovered, serial_port) is not None


_SO101_RUST_PROBE = SO101RustProbe()


def _definitions() -> list[RobotCatalogDefinition]:
    """Return the Rust-backed SO-101 catalog definitions."""
    return [
        RobotCatalogDefinition(
            type="SO101Rust_Follower",
            display_name="SO101 Rust Follower",
            role="follower",
            robot_builder=_build_so101_rust_driver,
            robot_payload=SO101RustRobotPayload,
            asset=None,
            probe=_SO101_RUST_PROBE,
        ),
        RobotCatalogDefinition(
            type="SO101Rust_Leader",
            display_name="SO101 Rust Leader",
            role="leader",
            robot_builder=_build_so101_rust_driver,
            robot_payload=SO101RustLeaderPayload,
            asset=None,
            probe=_SO101_RUST_PROBE,
        ),
    ]


def register_physicalai_studio_plugin(registry: _RobotCatalogRegistry) -> None:
    """Register all Rust-backed SO-101 catalog entries with Physical AI Studio."""
    for definition in _definitions():
        registry.register_robot(definition)
