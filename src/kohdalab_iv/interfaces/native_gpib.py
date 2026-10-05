"""USB-GPIB transport through a separate, persistent GPL helper process."""

from __future__ import annotations

import os
import re
import queue
import select
import subprocess
import sys
import time
from pathlib import Path
from threading import RLock, Thread

_LOCK = RLock()
_BRIDGES: dict[str, _Bridge] = {}


def helper_path() -> Path | None:
    override = os.environ.get("KOHDALAB_IV_GPIB_HELPER")
    if override:
        return Path(override).expanduser()
    if sys.platform in ("darwin", "win32") and getattr(sys, "frozen", False):
        suffix = ".exe" if sys.platform == "win32" else ""
        return Path(sys._MEIPASS) / f"kohdalab-gpib-helper{suffix}"  # type: ignore[attr-defined]
    return None


def enabled() -> bool:
    path = helper_path()
    return path is not None and path.is_file()


def list_resources() -> tuple[str, ...]:
    path = helper_path()
    if path is None or not path.is_file():
        raise RuntimeError("USB-GPIB helper is missing.")
    with _LOCK:
        bridge = _BRIDGES.get(str(path.resolve()))
        if bridge is not None and bridge.process.poll() is None:
            return tuple(bridge.request("LIST", 1, 10000).splitlines())
        result = subprocess.run(
            [str(path), "--available"],
            capture_output=True,
            text=True,
            timeout=10,
            check=True,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
        if result.stdout.strip() == "0":
            return ()
        if result.stdout.strip() != "1":
            raise RuntimeError("Invalid USB-GPIB discovery response.")
        temporary = _Bridge(path)
        try:
            return tuple(temporary.request("LIST", 1, 10000).splitlines())
        finally:
            try:
                temporary.request("EXIT", 1, 5000)
            finally:
                temporary.stop()


class _Bridge:
    def __init__(
        self, path: Path, arguments: tuple[str, ...] = (), label: str = "USB-GPIB"
    ):
        self.label = label
        self.process = subprocess.Popen(
            [str(path), *arguments],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            bufsize=0,
            creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
        )
        self.buffer = b""
        self.lock = RLock()
        self.references = 0
        self.chunks: queue.Queue[bytes | Exception] | None = None
        if sys.platform == "win32":
            # Windows select() accepts sockets, not subprocess pipe handles.
            self.chunks = queue.Queue()
            Thread(target=self._read_pipe, daemon=True).start()
        try:
            self._response(30)
        except Exception:
            self.stop()
            raise

    def _read_pipe(self) -> None:
        assert self.process.stdout is not None and self.chunks is not None
        try:
            while True:
                chunk = os.read(self.process.stdout.fileno(), 4096)
                self.chunks.put(chunk)
                if not chunk:
                    return
        except Exception as error:
            self.chunks.put(error)

    def _chunk(self, remaining: float) -> bytes:
        assert self.process.stdout is not None
        if remaining <= 0:
            raise TimeoutError
        if self.chunks is not None:
            try:
                item = self.chunks.get(timeout=remaining)
            except queue.Empty as error:
                raise TimeoutError from error
            if isinstance(item, Exception):
                raise RuntimeError(f"{self.label} pipe reader failed.") from item
            return item
        if not select.select([self.process.stdout], [], [], remaining)[0]:
            raise TimeoutError
        return os.read(self.process.stdout.fileno(), 4096)

    def _response(self, seconds: float) -> str:
        assert self.process.stdout is not None
        deadline = time.monotonic() + seconds
        while True:
            if b"\n" in self.buffer:
                line, self.buffer = self.buffer.split(b"\n", 1)
                if not line.startswith(b"KIV\t"):
                    continue
                _, status, data = line.decode("ascii").split("\t", 2)
                message = bytes.fromhex(data).decode("utf-8")
                if status != "OK":
                    raise RuntimeError(message)
                return message
            remaining = deadline - time.monotonic()
            try:
                chunk = self._chunk(remaining)
            except TimeoutError:
                self.stop()
                raise TimeoutError(
                    f"{self.label} helper stopped responding; reconnect devices."
                ) from None
            if not chunk:
                raise RuntimeError(
                    f"{self.label} helper exited unexpectedly; reconnect devices."
                )
            self.buffer += chunk

    def request(
        self, operation: str, address: int, timeout: int, data: str = ""
    ) -> str:
        with self.lock:
            if self.process.poll() is not None:
                raise RuntimeError(f"{self.label} connection is closed.")
            assert self.process.stdin is not None
            payload = data.encode("utf-8").hex() or "-"
            self.process.stdin.write(
                f"{operation} {address} {timeout} {payload}\n".encode()
            )
            return self._response(timeout / 1000 * 4 + 10)

    def stop(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            self.process.wait(timeout=5)
        for stream in (self.process.stdin, self.process.stdout):
            if stream is not None:
                stream.close()


class NativeGpibHandle:
    def __init__(self, resource: str, timeout_ms: int = 5000):
        match = re.fullmatch(r"GPIB0::(\d+)::INSTR", resource, re.IGNORECASE)
        if match is None or not 1 <= int(match.group(1)) <= 30:
            raise ValueError("Native USB-GPIB requires GPIB0::<1..30>::INSTR.")
        path = helper_path()
        if path is None or not path.is_file():
            raise RuntimeError("USB-GPIB helper is missing.")
        self.address = int(match.group(1))
        self.timeout = int(timeout_ms)
        self.read_termination = "\n"
        self.write_termination = "\n"
        self.closed = False
        self.key = str(path.resolve())
        with _LOCK:
            bridge = _BRIDGES.get(self.key)
            if bridge is None or bridge.process.poll() is not None:
                bridge = _Bridge(path)
                _BRIDGES[self.key] = bridge
            self.bridge = bridge
            bridge.references += 1

    @property
    def session(self) -> int | None:
        return (
            None
            if self.closed or self.bridge.process.poll() is not None
            else self.address
        )

    def _request(self, operation: str, data: str = "") -> str:
        if self.closed:
            raise RuntimeError("USB-GPIB resource is closed.")
        return self.bridge.request(operation, self.address, self.timeout, data)

    def write(self, command: str) -> None:
        self._request("WRITE", command.rstrip("\r\n") + self.write_termination)

    def query(self, command: str) -> str:
        return self._request("QUERY", command.rstrip("\r\n") + self.write_termination)

    def read(self) -> str:
        return self._request("READ")

    def clear(self) -> None:
        self._request("CLEAR")

    def control_ren(self, mode: int) -> None:
        if int(mode) == 6:
            self._request("LOCAL")
        elif int(mode) in (0, 2):
            self._request("REN", "0")
        else:
            raise ValueError(f"Unsupported native GPIB REN mode: {mode}")

    def close(self) -> None:
        with _LOCK:
            if self.closed:
                return
            self.closed = True
            self.bridge.references -= 1
            if self.bridge.references == 0:
                if _BRIDGES.get(self.key) is self.bridge:
                    _BRIDGES.pop(self.key)
                try:
                    self.bridge.request("EXIT", self.address, self.timeout)
                finally:
                    self.bridge.stop()


def release_remote(board: str) -> None:
    if board.upper() == "GPIB0":
        with _LOCK:
            for bridge in _BRIDGES.values():
                bridge.request("REN", 1, 5000, "0")
