from __future__ import annotations

from typing import Any


def open_visa(resource: str, timeout_ms: int = 5000) -> Any:
    import pyvisa

    rm = pyvisa.ResourceManager()
    inst: Any = rm.open_resource(resource)
    inst.timeout = int(timeout_ms)
    inst.write_termination = "\n"
    inst.read_termination = "\n"
    return inst


def list_visa_resources() -> tuple[str, ...]:
    import pyvisa

    rm = pyvisa.ResourceManager()
    try:
        return tuple(rm.list_resources())
    finally:
        rm.close()
