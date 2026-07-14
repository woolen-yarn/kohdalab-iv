from __future__ import annotations

from dataclasses import dataclass
from enum import Enum


class MeasurementPhase(str, Enum):
    IDLE = "idle"
    RUNNING = "running"
    STOPPING = "stopping"


@dataclass
class MeasurementRunState:
    phase: MeasurementPhase = MeasurementPhase.IDLE
    last_error: str | None = None
    points_collected: int = 0

    @property
    def active(self) -> bool:
        return self.phase is not MeasurementPhase.IDLE

    @property
    def controls_enabled(self) -> bool:
        return not self.active

    @property
    def can_stop(self) -> bool:
        return self.phase is MeasurementPhase.RUNNING

    def begin(self) -> None:
        if self.active:
            raise RuntimeError("A measurement is already running.")
        self.phase = MeasurementPhase.RUNNING
        self.last_error = None
        self.points_collected = 0

    def request_stop(self) -> bool:
        if not self.can_stop:
            return False
        self.phase = MeasurementPhase.STOPPING
        return True

    def record_error(self, message: str) -> None:
        if not self.active:
            raise RuntimeError("Cannot record a measurement error while idle.")
        self.last_error = str(message)

    def finish(self, points_collected: int) -> None:
        if points_collected < 0:
            raise ValueError("points_collected must be non-negative.")
        self.points_collected = int(points_collected)
        self.phase = MeasurementPhase.IDLE

    def reset(self) -> None:
        self.phase = MeasurementPhase.IDLE
        self.last_error = None
        self.points_collected = 0
