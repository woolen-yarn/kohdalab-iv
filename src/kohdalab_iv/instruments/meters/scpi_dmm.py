from __future__ import annotations

from statistics import fmean

from kohdalab_iv.instruments.visa_base import VisaDevice


class ScpiDMM(VisaDevice):
    def local(self) -> None:
        self.gpib_go_to_local()

    def configure_measurement(
        self, *, measure_function: str, nplc: float, auto_range: bool = True
    ) -> None:
        if measure_function == "dc_voltage":
            self.write("CONF:VOLT:DC")
            if auto_range:
                self.write("SENS:VOLT:DC:RANG:AUTO ON")
            self.write(f"SENS:VOLT:DC:NPLC {float(nplc):.12g}")
        elif measure_function == "dc_current":
            self.write("CONF:CURR:DC")
            if auto_range:
                self.write("SENS:CURR:DC:RANG:AUTO ON")
            self.write(f"SENS:CURR:DC:NPLC {float(nplc):.12g}")
        else:
            raise ValueError(f"Unsupported DMM measure function: {measure_function}")

    def _try_write(self, command: str) -> None:
        try:
            self.write(command)
        except Exception:
            pass

    def read_once(self) -> float:
        return self.query_float("READ?")

    def read_average(self, count: int) -> float:
        values = [self.read_once() for _ in range(max(1, int(count)))]
        return float(fmean(values))

    def clear_status(self) -> None:
        self.write("*CLS")

    def error_status(self) -> str:
        return self.query("SYST:ERR?")

    def connect_status(self) -> str:
        self.clear_status()
        return self.error_status()


AgilentDMM = ScpiDMM
