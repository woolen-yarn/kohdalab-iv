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
    SRATE_BY_MAX_NPLC = (
        (0.02, "FAST"),
        (0.2, "MED"),
        (1.0, "SLOW"),
        (float("inf"), "SSLOW"),
    )
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
        self._scpi_measure_function = "VOLTAGE:DC"

    def local(self) -> None:
        self.prepare_for_disconnect()
        if self._uses_gpib:
            self.gpib_send_go_to_local()
            self.gpib_go_to_local()
        elif self._uses_usb:
            self.usb_go_to_local()
            self.usb_deassert_ren()

    def local_after_close(self) -> None:
        pass

    def prepare_for_disconnect(self) -> None:
        self.clear()
        if self._uses_scpi:
            self._try_scpi_setting(":ABORt")
        else:
            self._try_write("H0")
        self._try_scpi_setting("*CLS") if self._uses_scpi else self._try_write("*CLS")

    def configure_measurement(self, *, measure_function: str, nplc: float, auto_range: bool = True) -> None:
        if self._uses_scpi:
            self._configure_measurement_scpi(measure_function=measure_function, nplc=nplc, auto_range=auto_range)
            return
        self._configure_measurement_adc(measure_function=measure_function, nplc=nplc, auto_range=auto_range)

    def _configure_measurement_scpi(self, *, measure_function: str, nplc: float, auto_range: bool) -> None:
        function = self.SCPI_FUNCTION_COMMANDS.get(measure_function)
        if function is None:
            raise ValueError(f"Unsupported DMM measure function: {measure_function}")

        self._scpi_measure_function = function
        if self._uses_usb:
            return

        self._drain_scpi_errors()
        self._write_scpi_setting("*RST")
        self._write_scpi_setting(f":SENSE:FUNCTION '{function}'")
        self._write_scpi_setting(f":SENSE:{function}:SRATE {self._sampling_rate(nplc)}")

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
            if self._uses_usb:
                return self.query_float(f":MEASure:{self._scpi_measure_function}?")
            return self.query_float("READ?")
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
        if self._uses_scpi and self._uses_usb:
            return "status cleared"
        if self._uses_scpi:
            return self._drain_scpi_errors()
        return self.error_status()

    @property
    def _uses_scpi(self) -> bool:
        return self.command_language == "scpi"

    @property
    def _uses_gpib(self) -> bool:
        return self.resource.strip().upper().startswith("GPIB")

    @property
    def _uses_usb(self) -> bool:
        return self.resource.strip().upper().startswith("USB")

    def _try_write(self, command: str) -> None:
        try:
            self.write(command)
        except Exception:
            pass

    def _write_scpi_setting(self, command: str) -> str | None:
        self.write(command)
        if self._uses_usb:
            return None
        return self._drain_scpi_errors()

    def _try_scpi_setting(self, command: str) -> str | None:
        try:
            return self._write_scpi_setting(command)
        except Exception:
            return None

    def _drain_scpi_errors(self, *, max_reads: int = 20) -> str:
        first_problem = None
        last_status = "0,No error"
        for _ in range(max(1, int(max_reads))):
            status = self.error_status()
            last_status = status
            if self._is_no_error(status):
                return first_problem or status
            if first_problem is None and not self._is_undefined_header(status):
                first_problem = status
        return first_problem or last_status

    def _is_no_error(self, status: str) -> bool:
        normalized = str(status).strip().lower()
        return normalized.startswith("0") or "no error" in normalized

    def _is_undefined_header(self, status: str) -> bool:
        normalized = str(status).strip().lower()
        return "undefined header" in normalized or "err-113" in normalized or "-113" in normalized

    def _sampling_rate(self, nplc: float) -> str:
        value = float(nplc)
        for limit, rate in self.SRATE_BY_MAX_NPLC:
            if value <= limit:
                return rate
        return "SSLOW"
