# Copyright (C) 2026 Intel Corporation
# SPDX-License-Identifier: Apache-2.0

"""Rust-backed UVC camera.

The native extension owns camera capture and RGB decoding; this module adapts
it onto the :class:`~physicalai.capture.Camera` abstract base class.
"""

from __future__ import annotations

import time

from physicalai.capture import Camera, ColorMode
from physicalai.capture.frame import Frame
from physicalai.config import export_config


@export_config(class_path="physicalai_so101_rust_plugin.SO101RustUVCCamera")
class SO101RustUVCCamera(Camera):
    """UVC camera backed by the Rust nokhwa driver.

    Args:
        device: Camera index or device selector (``int`` index, or a string
            like ``"/dev/video0"``).
        width: Requested frame width.
        height: Requested frame height.
        fps: Requested frames per second.
        color_mode: Pixel format for returned frames.
    """

    def __init__(
        self,
        *,
        device: int | str = 0,
        width: int = 640,
        height: int = 480,
        fps: int = 30,
        color_mode: ColorMode | str = ColorMode.RGB,
    ) -> None:
        super().__init__(color_mode=color_mode)

        from physicalai_so101_rust_plugin import _native

        self._index = _device_index(device)
        self._device_id = f"/dev/video{self._index}"
        self._native = _native.RustCamera(self._index, width, height, fps)
        self._sequence = 0

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    def connect(self, timeout: float = 5.0) -> None:
        """Open the camera and start the stream."""
        _ = timeout
        self._native.connect()

    def _do_disconnect(self) -> None:
        """Stop the stream and release the camera."""
        self._native.disconnect()

    @property
    def is_connected(self) -> bool:
        """Whether the camera stream is open."""
        return self._native.is_connected()

    @property
    def device_id(self) -> str:
        """Identifier for the physical device this instance targets."""
        return self._device_id

    # ------------------------------------------------------------------
    # Reading
    # ------------------------------------------------------------------

    def read(self, timeout: float = 2.0) -> Frame:
        """Read the next frame as an ``(H, W, 3)`` uint8 RGB array."""
        _ = timeout
        return self._make_frame(self._native.read())

    def read_latest(self) -> Frame:
        """Read the most recent frame (best-effort)."""
        return self._make_frame(self._native.read_latest())

    def _make_frame(self, data) -> Frame:
        self._sequence += 1
        return Frame(data=data, timestamp=time.monotonic(), sequence=self._sequence)


def _device_index(device: int | str) -> int:
    if isinstance(device, int):
        return device
    if isinstance(device, str) and device.isdecimal():
        return int(device)
    if isinstance(device, str) and device.startswith("/dev/video"):
        return int(device[len("/dev/video") :])
    msg = f"Unsupported device selector {device!r}; use an int index or '/dev/videoN'"
    raise ValueError(msg)
