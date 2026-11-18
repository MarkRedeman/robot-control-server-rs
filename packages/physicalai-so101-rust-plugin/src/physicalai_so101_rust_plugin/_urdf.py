# Copyright (C) 2026 Intel Corporation
# SPDX-License-Identifier: Apache-2.0

"""URDF path utility for bundled SO-101 descriptions."""

from __future__ import annotations

from pathlib import Path

_URDF_DIR = Path(__file__).parent / "urdf"


def get_urdf_path() -> Path:
    """Return the directory containing bundled SO-101 URDF assets."""
    return _URDF_DIR
