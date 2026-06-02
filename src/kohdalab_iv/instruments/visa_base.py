from __future__ import annotations

import re
import time
from typing import Any

from kohdalab_iv.interfaces.common import open_visa

_FLOAT_RE = re.compile(r"[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[Ee][-+]?\d+)?")


class VisaDevice:
    def __init__(self, resource: str, *, timeout_ms: int = 5000, handle: Any | None = None):
        self.resource = resource
        self.timeout_ms = int(timeout_ms)
        self.inst = handle if handle is not None else open_visa(resource, timeout_ms=self.timeout_ms)

    def write(self, command: str) -> None:
        self.inst.write(command)

    def query(self, command: str, *, delay_s: float = 0.0) -> str:
        if delay_s:
            self.inst.write(command)
            time.sleep(delay_s)
            return str(self.inst.read()).strip()
        return str(self.inst.query(command)).strip()

    def query_float(self, command: str) -> float:
        response = self.query(command)
        match = _FLOAT_RE.search(response)
        if not match:
            raise RuntimeError(f"Unexpected numeric response for {command}: {response!r}")
        return float(match.group(0))

    def identify(self) -> str:
        return self.query("*IDN?")

    def clear(self) -> None:
        try:
            self.inst.clear()
        except Exception:
            pass

    def local(self) -> None:
        try:
            self.write("SYST:LOC")
        except Exception:
            pass
        self.gpib_go_to_local()

    def gpib_go_to_local(self, *, release_ren: bool = False) -> None:
        try:
            from pyvisa import constants

            mode = constants.VI_GPIB_REN_DEASSERT_GTL if release_ren else constants.VI_GPIB_REN_ADDRESS_GTL
            self.inst.control_ren(mode)
        except Exception:
            pass

    def close(self) -> None:
        self.inst.close()

    def is_connected(self) -> bool:
        return getattr(self.inst, "session", None) is not None
