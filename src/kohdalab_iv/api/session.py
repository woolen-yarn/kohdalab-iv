from __future__ import annotations

from threading import RLock
from typing import Any

from kohdalab_iv.api.config import instrument_config
from kohdalab_iv.instruments.meters.adcmt_7461a import ADCMT7461A
from kohdalab_iv.instruments.meters.agilent_34401a import Agilent34401A
from kohdalab_iv.instruments.meters.agilent_34411a import Agilent34411A
from kohdalab_iv.instruments.meters.keysight_34411a import Keysight34411A
from kohdalab_iv.instruments.meters.keysight_34465a import Keysight34465A
from kohdalab_iv.instruments.sources.gs210 import YokogawaGS210
from kohdalab_iv.instruments.visa_base import gpib_board_from_resource, release_gpib_remote


SOURCE_CONTROLLERS = {
    "YOKOGAWA_GS210": YokogawaGS210,
}

METER_CONTROLLERS = {
    "AGILENT_34401A": Agilent34401A,
    "AGILENT_34411A": Agilent34411A,
    "KEYSIGHT_34411A": Keysight34411A,
    "KEYSIGHT_34465A": Keysight34465A,
    "ADCMT_7461A": ADCMT7461A,
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
        kind, key = self._split_ref(ref)
        cfg = instrument_config(self.config, ref)
        model = str(cfg["model"]).strip().upper()
        cls = self._controller_for(kind, model)
        resource = str(cfg["resource"])

        existing = self._get_device(kind, key)
        if existing is not None:
            if self._can_reuse_device(existing, cls=cls, resource=resource):
                return existing
            self.disconnect_device(ref)

        device = cls(resource, **self._controller_kwargs(model, cfg))
        self._set_device(kind, key, device)
        return device

    def _controller_kwargs(self, model: str, cfg: dict[str, Any]) -> dict[str, Any]:
        kwargs: dict[str, Any] = {"timeout_ms": int(cfg.get("timeout_ms", 5000))}
        if model == "ADCMT_7461A":
            kwargs["command_language"] = str(cfg.get("command_language", "scpi"))
        return kwargs

    def connect_all(self) -> None:
        for kind in ("source", "meter"):
            for key in self._configured_keys(kind):
                self.connect_device(f"{kind}.{key}")

    def disconnect_device(self, ref: str) -> list[str]:
        kind, key = self._split_ref(ref)
        device = self._get_device(kind, key)
        if device is None:
            return []

        board = gpib_board_from_resource(getattr(device, "resource", ""))
        if board is None:
            self._disconnect_device(kind, key, release_board=False)
            return [ref]

        targets = self._connected_refs_on_gpib_board(board, first=(kind, key))
        for target_kind, target_key in targets:
            self._disconnect_device(target_kind, target_key, release_board=False)
        release_gpib_remote(board)
        return [f"{target_kind}.{target_key}" for target_kind, target_key in targets]

    def _disconnect_device(self, kind: str, key: str, *, release_board: bool) -> None:
        device = self._pop_device(kind, key)
        if device is not None:
            board = gpib_board_from_resource(getattr(device, "resource", ""))
            self._return_and_close_device(device)
            if release_board and board is not None:
                release_gpib_remote(board)

    def disconnect_all(self) -> None:
        boards = {
            board
            for kind in ("source", "meter")
            for device in self._devices_snapshot(kind)
            if (board := gpib_board_from_resource(getattr(device, "resource", ""))) is not None
        }
        for kind in ("source", "meter"):
            for key in self._connected_keys(kind):
                self._disconnect_device(kind, key, release_board=False)
        for board in sorted(boards):
            release_gpib_remote(board)

    def connected_devices(self) -> dict[str, bool]:
        connected: dict[str, bool] = {}
        for kind in ("source", "meter"):
            for key in self._configured_keys(kind):
                connected[f"{kind}.{key}"] = self._connection_state(kind, key)
        return connected

    def require(self, ref: str):
        kind, key = self._split_ref(ref)
        device = self._get_device(kind, key)
        if device is not None and self._device_is_connected(device):
            return device
        if device is not None:
            self._discard_device(kind, key, device, close=True)
        if not self.auto_connect:
            raise RuntimeError(f"Device not connected: {ref}")
        return self.connect_device(ref)

    def _controller_for(self, kind: str, model: str):
        if kind == "source":
            cls = SOURCE_CONTROLLERS.get(model)
        elif kind == "meter":
            cls = METER_CONTROLLERS.get(model)
        else:
            raise ValueError(f"Unsupported device kind: {kind}")
        if cls is None:
            raise ValueError(f"Unsupported {kind} model: {model}")
        return cls

    def _can_reuse_device(self, device, *, cls, resource: str) -> bool:
        return (
            isinstance(device, cls)
            and getattr(device, "resource", None) == resource
            and self._device_is_connected(device)
        )

    def _connection_state(self, kind: str, key: str) -> bool:
        device = self._get_device(kind, key)
        if device is None:
            return False
        if self._device_is_connected(device):
            return True
        self._discard_device(kind, key, device, close=False)
        return False

    def _return_and_close_device(self, device) -> None:
        self._safe_output_off(device)
        self._call_if_present(device, "local")
        self._call_if_present(device, "close")
        self._call_if_present(device, "local_after_close")

    def _device_is_connected(self, device) -> bool:
        if hasattr(device, "is_connected"):
            try:
                return bool(device.is_connected())
            except Exception:
                return False
        return True

    def _discard_device(self, kind: str, key: str, device, *, close: bool) -> None:
        with self._lock:
            if self._map(kind).get(key) is device:
                self._map(kind).pop(key, None)
        if close:
            self._return_and_close_device(device)

    def _call_if_present(self, device, method_name: str) -> None:
        method = getattr(device, method_name, None)
        if method is None:
            return
        try:
            method()
        except Exception:
            pass

    def _safe_output_off(self, device) -> None:
        output_off = getattr(device, "output_off", None)
        set_level = getattr(device, "set_level", None)
        if output_off is None and set_level is None:
            return
        self._call_if_present(device, "output_off")
        if set_level is not None:
            try:
                set_level(0.0)
            except Exception:
                pass
        self._call_if_present(device, "output_off")

    def _configured_keys(self, kind: str) -> list[str]:
        devices = self.config.get("instruments", {}).get(kind, {})
        return list(devices) if isinstance(devices, dict) else []

    def _connected_keys(self, kind: str) -> list[str]:
        with self._lock:
            return list(self._map(kind))

    def _devices_snapshot(self, kind: str) -> list[Any]:
        with self._lock:
            return list(self._map(kind).values())

    def _items_snapshot(self, kind: str) -> list[tuple[str, Any]]:
        with self._lock:
            return list(self._map(kind).items())

    def _connected_refs_on_gpib_board(self, board: str, *, first: tuple[str, str]) -> list[tuple[str, str]]:
        targets = [
            (kind, key)
            for kind in ("source", "meter")
            for key, device in self._items_snapshot(kind)
            if gpib_board_from_resource(getattr(device, "resource", "")) == board
        ]
        return sorted(targets, key=lambda item: 0 if item == first else 1)

    def _get_device(self, kind: str, key: str):
        with self._lock:
            return self._map(kind).get(key)

    def _set_device(self, kind: str, key: str, device) -> None:
        with self._lock:
            self._map(kind)[key] = device

    def _pop_device(self, kind: str, key: str):
        with self._lock:
            return self._map(kind).pop(key, None)

    def _split_ref(self, ref: str) -> tuple[str, str]:
        try:
            kind, key = ref.split(".", 1)
        except ValueError as e:
            raise ValueError(f"Invalid device ref: {ref}") from e
        if not kind or not key:
            raise ValueError(f"Invalid device ref: {ref}")
        return kind, key

    def _map(self, kind: str) -> dict[str, Any]:
        if kind == "source":
            return self.sources
        if kind == "meter":
            return self.meters
        raise ValueError(f"Unsupported device kind: {kind}")
