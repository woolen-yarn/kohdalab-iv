from __future__ import annotations

import re

from kohdalab_iv.instruments.meters.agilent_dmm import AgilentDMM


class ADCMT7461A(AgilentDMM):
    FUNCTION_COMMANDS = {
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

    def local(self) -> None:
        self.release_remote_control()

    def local_after_close(self) -> None:
        self.gpib_interface_go_to_local(release_ren=True)

    def configure_measurement(self, *, measure_function: str, nplc: float, auto_range: bool = True) -> None:
        function = self.FUNCTION_COMMANDS.get(measure_function)
        if function is None:
            raise ValueError(f"Unsupported DMM measure function: {measure_function}")

        self._write_checked(f":SENSE:FUNCTION '{function}'")
        if auto_range:
            self._write_checked(f":SENSE:{function}:RANGE:AUTO ON")
        self._write_checked(f":SENSE:{function}:SRATE {self._sampling_rate(nplc)}")

    def read_once(self) -> float:
        return self.query_float(":READ?", delay_s=self.USB_QUERY_DELAY_S)

    def _write_checked(self, command: str) -> None:
        self.write(command)
        error = self.query(":SYSTem:ERRor?")
        match = re.match(r"\s*([+-]?\d+)", error)
        if match is not None and int(match.group(1)) != 0:
            raise RuntimeError(f"ADCMT 7461A command error after {command}: {error}")

    def _sampling_rate(self, nplc: float) -> str:
        value = float(nplc)
        for limit, rate in self.SRATE_BY_MAX_NPLC:
            if value <= limit:
                return rate
        return "SSLOW"
