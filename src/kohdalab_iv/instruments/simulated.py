from __future__ import annotations

from dataclasses import dataclass
from threading import RLock


@dataclass
class _Circuit:
    level: float = 0.0
    source_function: str = "voltage"
    output_enabled: bool = False
    resistance_ohm: float = 1000.0


_CIRCUITS: dict[str, _Circuit] = {}
_LOCK = RLock()


def _circuit(resource: str) -> _Circuit:
    with _LOCK:
        return _CIRCUITS.setdefault(resource, _Circuit())


def reset_simulated_circuits() -> None:
    with _LOCK:
        _CIRCUITS.clear()


class SimulatedSource:
    def __init__(self, resource: str, *, timeout_ms: int = 5000) -> None:
        del timeout_ms
        self.resource = resource
        self._connected = True
        circuit = _circuit(resource)
        circuit.level = 0.0
        circuit.output_enabled = False

    def configure_source(
        self,
        *,
        source_function: str,
        source_range: float,
        hardware_compliance: float,
    ) -> None:
        del source_range, hardware_compliance
        if source_function not in {"voltage", "current"}:
            raise ValueError(
                f"Unsupported simulated source function: {source_function}"
            )
        _circuit(self.resource).source_function = source_function

    def output_on(self) -> None:
        _circuit(self.resource).output_enabled = True

    def output_off(self) -> None:
        circuit = _circuit(self.resource)
        circuit.output_enabled = False
        circuit.level = 0.0

    def set_level(self, value: float) -> None:
        _circuit(self.resource).level = float(value)

    def read_level(self) -> float:
        circuit = _circuit(self.resource)
        return circuit.level if circuit.output_enabled else 0.0

    def local(self) -> None:
        return None

    def close(self) -> None:
        self.output_off()
        self._connected = False

    def is_connected(self) -> bool:
        return self._connected


class SimulatedMeter:
    def __init__(
        self,
        resource: str,
        *,
        timeout_ms: int = 5000,
        resistance_ohm: float = 1000.0,
        offset: float = 0.0,
    ) -> None:
        del timeout_ms
        if resistance_ohm <= 0:
            raise ValueError("Simulated resistance_ohm must be positive.")
        self.resource = resource
        self.offset = float(offset)
        self.measure_function = "dc_current"
        self._connected = True
        _circuit(resource).resistance_ohm = float(resistance_ohm)

    def configure_measurement(
        self, *, measure_function: str, nplc: float, auto_range: bool
    ) -> None:
        del nplc, auto_range
        if measure_function not in {"dc_voltage", "dc_current"}:
            raise ValueError(
                f"Unsupported simulated measure function: {measure_function}"
            )
        self.measure_function = measure_function

    def prepare_for_reading(self) -> None:
        return None

    def read_average(self, count: int) -> float:
        if count < 1:
            raise ValueError("count must be >= 1.")
        circuit = _circuit(self.resource)
        level = circuit.level if circuit.output_enabled else 0.0
        if self.measure_function == "dc_current":
            value = level / circuit.resistance_ohm
        else:
            value = level * circuit.resistance_ohm
        return value + self.offset

    def local(self) -> None:
        return None

    def close(self) -> None:
        self._connected = False

    def is_connected(self) -> bool:
        return self._connected
