from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class MeasurementPoint:
    index: int
    total_points: int
    row: dict[str, Any]
