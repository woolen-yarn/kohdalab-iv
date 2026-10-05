"""Direct GS210/34411A USBTMC through a bundled libusb helper."""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

from kohdalab_iv.interfaces.native_gpib import _Bridge


def helper_path() -> Path | None:
    override = os.environ.get("KOHDALAB_IV_USB_HELPER")
    if override:
        return Path(override).expanduser()
    if sys.platform in ("darwin", "win32") and getattr(sys, "frozen", False):
        suffix = ".exe" if sys.platform == "win32" else ""
        return Path(sys._MEIPASS) / f"kohdalab-usbtmc-helper{suffix}"  # type: ignore[attr-defined]
    return None


def enabled() -> bool:
    path = helper_path()
    return path is not None and path.is_file()


def list_resources() -> tuple[str, ...]:
    path = helper_path()
    if path is None or not path.is_file():
        raise RuntimeError("USBTMC helper is missing.")
    result = subprocess.run(
        [str(path), "--list"],
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
        creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
    )
    line = result.stdout.rstrip("\r\n")
    if not line.startswith("KIV\t"):
        raise RuntimeError("USB discovery returned an invalid response.")
    _, status, encoded = line.split("\t", 2)
    message = bytes.fromhex(encoded).decode("utf-8")
    if status != "OK" or result.returncode != 0:
        raise RuntimeError(message or "USB discovery failed.")
    return tuple(dict.fromkeys(message.splitlines()))


class NativeUsbHandle:
    def __init__(self, resource: str, timeout_ms: int = 5000):
        match = re.fullmatch(
            r"USB0::(0[xX][0-9a-fA-F]+|[0-9]+)::"
            r"(0[xX][0-9a-fA-F]+|[0-9]+)::([A-Za-z0-9_-]+)::INSTR",
            resource,
            re.IGNORECASE,
        )
        if match is None or (
            int(match[1], 16 if match[1].lower().startswith("0x") else 10),
            int(match[2], 16 if match[2].lower().startswith("0x") else 10),
        ) not in {(0x0957, 0x0A07), (0x0B21, 0x0039)}:
            raise ValueError(
                "Native USB supports 34411A (0957:0A07) and GS210 (0B21:0039)."
            )
        path = helper_path()
        if path is None or not path.is_file():
            raise RuntimeError("USBTMC helper is missing.")
        self.timeout = int(timeout_ms)
        self.read_termination = "\n"
        self.write_termination = "\n"
        self.closed = False
        ids = tuple(
            int(match[i], 16 if match[i].lower().startswith("0x") else 10)
            for i in (1, 2)
        )
        self.bridge = _Bridge(path, (match[3], hex(ids[0]), hex(ids[1])), "USBTMC")

    @property
    def session(self) -> int | None:
        return None if self.closed or self.bridge.process.poll() is not None else 0

    def _request(self, operation: str, data: str = "") -> str:
        if self.closed:
            raise RuntimeError("USBTMC resource is closed.")
        return self.bridge.request(operation, 0, self.timeout, data)

    def write(self, command: str) -> None:
        self._request("WRITE", command.rstrip("\r\n") + self.write_termination)

    def query(self, command: str) -> str:
        return self._request("QUERY", command.rstrip("\r\n") + self.write_termination)

    def read(self) -> str:
        return self._request("READ")

    def clear(self) -> None:
        # No silent SCPI reset: this helper has no USBTMC clear implementation.
        raise NotImplementedError("USB device clear is not supported by this helper.")

    def control_ren(self, mode: int) -> None:
        if int(mode) == 6:
            self._request("LOCAL")
        elif int(mode) in (0, 2):
            self._request("REN", "0")
        else:
            raise ValueError(f"Unsupported native USB REN mode: {mode}")

    def close(self) -> None:
        if self.closed:
            return
        self.closed = True
        try:
            self.bridge.request("EXIT", 0, self.timeout)
        finally:
            self.bridge.stop()
