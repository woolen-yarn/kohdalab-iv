from __future__ import annotations

import sys
from contextvars import ContextVar
from typing import Any, Callable


DISCOVERY_PROGRESS: ContextVar[Callable[[str], None] | None] = ContextVar(
    "instrument_discovery_progress", default=None
)


def _discovery_progress(message: str) -> None:
    callback = DISCOVERY_PROGRESS.get()
    if callback is not None:
        callback(message)


def open_visa(resource: str, timeout_ms: int = 5000) -> Any:
    from kohdalab_iv.interfaces import native_gpib, native_usb

    if resource.upper().startswith("GPIB") and native_gpib.enabled():
        return native_gpib.NativeGpibHandle(resource, timeout_ms)
    if resource.upper().startswith("USB") and native_usb.enabled():
        return native_usb.NativeUsbHandle(resource, timeout_ms)
    import pyvisa

    rm = pyvisa.ResourceManager()
    inst: Any = rm.open_resource(resource)
    inst.timeout = int(timeout_ms)
    inst.write_termination = "\n"
    inst.read_termination = "\n"
    return inst


def list_visa_resources() -> tuple[str, ...]:
    from kohdalab_iv.interfaces import native_gpib, native_usb

    usb_enabled, gpib_enabled = native_usb.enabled(), native_gpib.enabled()
    resources: tuple[str, ...] = ()
    if usb_enabled:
        _discovery_progress("Checking direct USB instruments (10s timeout)")
        resources = native_usb.list_resources()
    if gpib_enabled:
        _discovery_progress("Checking USB-GPIB adapters and GPIB addresses")
        resources += native_gpib.list_resources()
    native_enabled = gpib_enabled or usb_enabled
    if getattr(sys, "frozen", False) and usb_enabled and gpib_enabled:
        # The native portable owns USB/GPIB. An installed vendor VISA runtime
        # can block during its separate discovery and is unnecessary here.
        return tuple(dict.fromkeys(resources))
    _discovery_progress("Checking external VISA resources")
    import pyvisa

    try:
        rm = pyvisa.ResourceManager()
    except (ValueError, OSError):
        if native_enabled:
            # The native USB transport works without an external VISA runtime.
            return resources
        raise
    try:
        return tuple(dict.fromkeys((*resources, *rm.list_resources())))
    finally:
        rm.close()
