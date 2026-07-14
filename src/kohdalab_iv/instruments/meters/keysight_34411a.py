from __future__ import annotations

from kohdalab_iv.instruments.meters.scpi_dmm import ScpiDMM


class Keysight34411A(ScpiDMM):
    def local(self) -> None:
        self.release_remote_control()

    def local_after_close(self) -> None:
        self.gpib_interface_go_to_local(release_ren=True)

    def configure_measurement(
        self, *, measure_function: str, nplc: float, auto_range: bool = True
    ) -> None:
        super().configure_measurement(
            measure_function=measure_function,
            nplc=nplc,
            auto_range=auto_range,
        )
        zero_command = {
            "dc_voltage": "SENS:VOLT:DC:ZERO:AUTO ON",
            "dc_current": "SENS:CURR:DC:ZERO:AUTO ON",
        }[measure_function]
        self._try_write(zero_command)
        self._try_write("TRIG:SOUR IMM")
