from __future__ import annotations

from statistics import fmean

from kohdalab_iv.instruments.visa_base import VisaDevice


class AgilentDMM(VisaDevice):
    def local(self) -> None:
        self.gpib_go_to_local()

    def configure_measurement(self, *, measure_function: str, nplc: float, auto_range: bool = True) -> None:
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


class Agilent34401A(AgilentDMM):
    pass


class Keysight34411A(AgilentDMM):
    def local(self) -> None:
        self.gpib_interface_go_to_local(release_ren=True)
        self.gpib_send_go_to_local()
        self.gpib_go_to_local()
        self.usb_go_to_local()
        self.gpib_go_to_local(release_ren=True)
        self.usb_deassert_ren()
        self.gpib_deassert_ren()

    def local_after_close(self) -> None:
        self.gpib_interface_go_to_local(release_ren=True)

    def configure_measurement(self, *, measure_function: str, nplc: float, auto_range: bool = True) -> None:
        super().configure_measurement(
            measure_function=measure_function,
            nplc=nplc,
            auto_range=auto_range,
        )
        if measure_function == "dc_voltage":
            self._try_write("SENS:VOLT:DC:ZERO:AUTO ON")
        elif measure_function == "dc_current":
            self._try_write("SENS:CURR:DC:ZERO:AUTO ON")
        self._try_write("TRIG:SOUR IMM")


class Keysight34465A(Keysight34411A):
    pass
