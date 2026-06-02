from __future__ import annotations

import re
import time
from statistics import fmean

from kohdalab_iv.instruments.visa_base import VisaDevice

_FLOAT_RE = re.compile(r"[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[Ee][-+]?\d+)?")


class ADCMT7461A(VisaDevice):
    ADC_FUNCTION_COMMANDS = {
        "dc_voltage": "F1",
        "dc_current": "F5",
    }
    SCPI_FUNCTION_COMMANDS = {
        "dc_voltage": "VOLTAGE:DC",
        "dc_current": "CURRENT:DC",
    }
    USB_QUERY_DELAY_S = 0.02
    READ_DELAY_S = 0.02
    DISCARD_READINGS_AFTER_SETTLE = 1

    def __init__(
        self,
        resource: str,
        *,
        timeout_ms: int = 5000,
        handle=None,
        command_language: str = "scpi",
    ):
        super().__init__(resource, timeout_ms=timeout_ms, handle=handle)
        self.command_language = str(command_language or "scpi").strip().lower()

    def local(self) -> None:
        self.release_remote_control()

    def local_after_close(self) -> None:
        self.gpib_interface_go_to_local(release_ren=True)

    def configure_measurement(self, *, measure_function: str, nplc: float, auto_range: bool = True) -> None:
        if self._uses_scpi:
            self._configure_measurement_scpi(measure_function=measure_function, nplc=nplc, auto_range=auto_range)
            return
        self._configure_measurement_adc(measure_function=measure_function, nplc=nplc, auto_range=auto_range)

    def _configure_measurement_scpi(self, *, measure_function: str, nplc: float, auto_range: bool) -> None:
        function = self.SCPI_FUNCTION_COMMANDS.get(measure_function)
        if function is None:
            raise ValueError(f"Unsupported DMM measure function: {measure_function}")

        self.write("*RST")
        self.write(f":SENSE:FUNCTION '{function}'")
        if auto_range:
            self._try_write(f":SENSE:{function}:RANGE:AUTO ON")
        self.write(f":SENSE:{function}:NPLCYCLES {float(nplc):.12g}")

    def _configure_measurement_adc(self, *, measure_function: str, nplc: float, auto_range: bool) -> None:
        function = self.ADC_FUNCTION_COMMANDS.get(measure_function)
        if function is None:
            raise ValueError(f"Unsupported DMM measure function: {measure_function}")

        self.write("*RST")
        self.write("H0")
        self.write(function)
        if auto_range:
            self.write("R0")
        self.write(f"ITP{float(nplc):.12g}")

    def read_once(self) -> float:
        if self._uses_scpi:
            return self.query_float(":FETCH?")
        time.sleep(self.READ_DELAY_S)
        response = str(self.inst.read()).strip()
        match = _FLOAT_RE.search(response)
        if not match:
            raise RuntimeError(f"Unexpected ADCMT 7461A measurement response: {response!r}")
        return float(match.group(0))

    def prepare_for_reading(self) -> None:
        for _ in range(max(0, int(self.DISCARD_READINGS_AFTER_SETTLE))):
            try:
                self.read_once()
            except Exception:
                break

    def read_average(self, count: int) -> float:
        values = [self.read_once() for _ in range(max(1, int(count)))]
        return float(fmean(values))

    def clear_status(self) -> None:
        self.write("*CLS")

    def error_status(self) -> str:
        if self._uses_scpi:
            return self.query(":SYSTem:ERRor?")
        return self.query("ERR?", delay_s=self.USB_QUERY_DELAY_S)

    def connect_status(self) -> str:
        self.clear_status()
        return self.error_status()

    @property
    def _uses_scpi(self) -> bool:
        return self.command_language == "scpi"

    def _try_write(self, command: str) -> None:
        try:
            self.write(command)
        except Exception:
            pass
