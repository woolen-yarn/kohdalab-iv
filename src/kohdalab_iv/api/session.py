from __future__ import annotations

from threading import RLock
from typing import Any

from kohdalab_iv.api.config import instrument_config
from kohdalab_iv.instruments.meters.agilent_dmm import Agilent34401A, Keysight34411A, Keysight34465A
from kohdalab_iv.instruments.sources.gs210 import YokogawaGS210
from kohdalab_iv.instruments.visa_base import gpib_board_from_resource, release_gpib_remote


SOURCE_CONTROLLERS = {
    "YOKOGAWA_GS210": YokogawaGS210,
}

METER_CONTROLLERS = {
    "AGILENT_34401A": Agilent34401A,
    "KEYSIGHT_34411A": Keysight34411A,
    "KEYSIGHT_34465A": Keysight34465A,
}


class DeviceSession:
    def __init__(self, config: dict[str, Any], *, auto_connect: bool = True):
        self.config = config
        self.auto_connect = bool(auto_connect)
        self.sources: dict[str, Any] = {}
        self.meters: dict[str, Any] = {}
        self._lock = RLock()

    def set_config(self, config: dict[str, Any]) -> None:
        with self._lock:
            self.config = config

    def connect_device(self, ref: str):
        kind, key = ref.split(".", 1)
        cfg = instrument_config(self.config, ref)
        model = str(cfg["model"]).strip().upper()
        cls = SOURCE_CONTROLLERS.get(model) if kind == "source" else METER_CONTROLLERS.get(model)
        if cls is None:
            raise ValueError(f"Unsupported {kind} model: {model}")
        device = cls(str(cfg["resource"]), timeout_ms=int(cfg.get("timeout_ms", 5000)))
        with self._lock:
            self._map(kind)[key] = device
        return device

    def connect_all(self) -> None:
        for kind in ("source", "meter"):
            devices = self.config.get("instruments", {}).get(kind, {})
            if isinstance(devices, dict):
                for key in devices:
                    self.connect_device(f"{kind}.{key}")

    def disconnect_device(self, ref: str) -> None:
        kind, key = ref.split(".", 1)
        with self._lock:
            device = self._map(kind).pop(key, None)
        if device is not None:
            try:
                if hasattr(device, "local"):
                    device.local()
            except Exception:
                pass
            try:
                device.close()
            except Exception:
                pass
            try:
                if hasattr(device, "local_after_close"):
                    device.local_after_close()
            except Exception:
                pass
            try:
                if hasattr(device, "close_resource_manager"):
                    device.close_resource_manager()
            except Exception:
                pass

    def disconnect_all(self) -> None:
        boards = {
            board
            for kind in ("source", "meter")
            for device in list(self._map(kind).values())
            if (board := gpib_board_from_resource(getattr(device, "resource", ""))) is not None
        }
        for kind in ("source", "meter"):
            for key in list(self._map(kind)):
                self.disconnect_device(f"{kind}.{key}")
        for board in sorted(boards):
            release_gpib_remote(board)

    def connected_devices(self) -> dict[str, bool]:
        connected: dict[str, bool] = {}
        for kind in ("source", "meter"):
            devices = self.config.get("instruments", {}).get(kind, {})
            if isinstance(devices, dict):
                for key in devices:
                    connected[f"{kind}.{key}"] = key in self._map(kind)
        return connected

    def require(self, ref: str):
        kind, key = ref.split(".", 1)
        with self._lock:
            device = self._map(kind).get(key)
        if device is not None:
            return device
        if not self.auto_connect:
            raise RuntimeError(f"Device not connected: {ref}")
        return self.connect_device(ref)

    def _map(self, kind: str) -> dict[str, Any]:
        if kind == "source":
            return self.sources
        if kind == "meter":
            return self.meters
        raise ValueError(f"Unsupported device kind: {kind}")
