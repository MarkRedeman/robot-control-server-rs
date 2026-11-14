# Copyright (C) 2026 Intel Corporation
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import numpy as np
from physicalai.capture import Camera, ColorMode
from physicalai.capture.frame import Frame

from physicalai_so101_rust_plugin.camera import SO101RustUVCCamera


def test_is_a_camera_subclass() -> None:
    camera = SO101RustUVCCamera(device=0)
    assert isinstance(camera, Camera)


def test_device_id_resolution() -> None:
    assert SO101RustUVCCamera(device=0).device_id == "/dev/video0"
    assert SO101RustUVCCamera(device="1").device_id == "/dev/video1"
    assert SO101RustUVCCamera(device="/dev/video2").device_id == "/dev/video2"


def test_connect_disconnect() -> None:
    camera = SO101RustUVCCamera(device=0)
    camera.connect()
    assert camera.is_connected
    camera.disconnect()
    assert not camera.is_connected


def test_read_returns_frame_with_incrementing_sequence() -> None:
    camera = SO101RustUVCCamera(device=0, width=4, height=2)
    camera.connect()
    first = camera.read()
    second = camera.read_latest()
    assert isinstance(first, Frame)
    assert first.data.shape == (2, 4, 3)
    assert first.data.dtype == np.uint8
    assert first.sequence == 1
    assert second.sequence == 2
    assert isinstance(first.timestamp, float)


def test_color_mode_is_stored() -> None:
    camera = SO101RustUVCCamera(device=0, color_mode=ColorMode.BGR)
    assert camera.color_mode is ColorMode.BGR
