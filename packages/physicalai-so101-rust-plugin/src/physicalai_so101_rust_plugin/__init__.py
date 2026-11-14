# Copyright (C) 2026 Intel Corporation
# SPDX-License-Identifier: Apache-2.0

"""Rust-backed SO-101 robot and camera for PhysicalAI."""

from __future__ import annotations

from importlib import import_module
from typing import TYPE_CHECKING

from physicalai_so101_rust_plugin._urdf import get_urdf_path as get_urdf_path

if TYPE_CHECKING:
    from physicalai_so101_rust_plugin.camera import SO101RustUVCCamera as SO101RustUVCCamera
    from physicalai_so101_rust_plugin.robot import SO101Rust as SO101Rust
    from physicalai_so101_rust_plugin.robot import SO101RustObservation as SO101RustObservation

__all__ = [
    "SO101Rust",
    "SO101RustObservation",
    "SO101RustUVCCamera",
    "get_urdf_path",
]


def __getattr__(name: str) -> object:
    """Lazily import the drivers so the native extension is only loaded on use.

    Returns:
        The requested class.

    Raises:
        AttributeError: If ``name`` is not exported by this package.
    """
    if name in {"SO101Rust", "SO101RustObservation"}:
        return getattr(import_module("physicalai_so101_rust_plugin.robot"), name)
    if name == "SO101RustUVCCamera":
        return getattr(import_module("physicalai_so101_rust_plugin.camera"), name)
    msg = f"module {__name__!r} has no attribute {name!r}"
    raise AttributeError(msg)
