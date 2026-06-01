from __future__ import annotations

from pathlib import Path
from typing import Any

from kohdalab_iv.api.config import load_config, normalize_config
from kohdalab_iv.api.measurements import ContinueCallback, PointCallback, StatusCallback, run_iv
from kohdalab_iv.api.scan_plan import IvPlan
from kohdalab_iv.api.session import DeviceSession


class Experiment:
    def __init__(self, config: dict[str, Any], *, auto_connect: bool = True):
        self._config = normalize_config(config)
        self.session = DeviceSession(self._config, auto_connect=auto_connect)

    @classmethod
    def from_config(cls, path: str | Path, *, auto_connect: bool = True) -> "Experiment":
        return cls(load_config(path), auto_connect=auto_connect)

    @property
    def config(self) -> dict[str, Any]:
        return self._config

    @config.setter
    def config(self, value: dict[str, Any]) -> None:
        self._config = normalize_config(value)
        self.session.set_config(self._config)

    def connect_all(self) -> None:
        self.session.connect_all()

    def connect_device(self, ref: str):
        return self.session.connect_device(ref)

    def disconnect_all(self) -> None:
        self.session.disconnect_all()

    def disconnect_device(self, ref: str) -> None:
        self.session.disconnect_device(ref)

    def connected_devices(self) -> dict[str, bool]:
        return self.session.connected_devices()

    def run_iv(
        self,
        *,
        plan: IvPlan | None = None,
        output: str | Path | None = None,
        on_point: PointCallback | None = None,
        on_status: StatusCallback | None = None,
        should_continue: ContinueCallback | None = None,
    ) -> list[dict[str, Any]]:
        return run_iv(
            self.config,
            plan=plan,
            output=output,
            on_point=on_point,
            on_status=on_status,
            should_continue=should_continue,
            session=self.session,
        )
