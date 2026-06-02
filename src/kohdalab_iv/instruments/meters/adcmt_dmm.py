from __future__ import annotations

from kohdalab_iv.instruments.meters.agilent_dmm import AgilentDMM


class ADCMT7461A(AgilentDMM):
    FUNCTION_COMMANDS = {
        "dc_voltage": "VOLTage:DC",
        "dc_current": "CURRent:DC",
    }
    USB_QUERY_DELAY_S = 0.02

    def local(self) -> None:
        self.release_remote_control()

    def local_after_close(self) -> None:
        self.gpib_interface_go_to_local(release_ren=True)

    def configure_measurement(self, *, measure_function: str, nplc: float, auto_range: bool = True) -> None:
        function = self.FUNCTION_COMMANDS.get(measure_function)
        if function is None:
            raise ValueError(f"Unsupported DMM measure function: {measure_function}")

        self.write(f"CONFigure:{function}")
        if auto_range:
            self._try_write(f"SENSe:{function}:RANGe:AUTO ON")
        self.write(f"SENSe:{function}:NPLCycles {float(nplc):.12g}")

    def read_once(self) -> float:
        return self.query_float("MEASure?", delay_s=self.USB_QUERY_DELAY_S)
