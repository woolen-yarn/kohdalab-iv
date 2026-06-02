from __future__ import annotations

import re
import time

from kohdalab_iv.instruments.meters.agilent_dmm import AgilentDMM


class ADCMT7461A(AgilentDMM):
    FUNCTION_COMMANDS = {
        "dc_voltage": "F1",
        "dc_current": "F5",
    }
    TRIGGER_DELAY_S = 0.02

    def local(self) -> None:
        self.release_remote_control()

    def local_after_close(self) -> None:
        self.gpib_interface_go_to_local(release_ren=True)

    def configure_measurement(self, *, measure_function: str, nplc: float, auto_range: bool = True) -> None:
        function = self.FUNCTION_COMMANDS.get(measure_function)
        if function is None:
            raise ValueError(f"Unsupported DMM measure function: {measure_function}")

        self.write("H0")
        self.write(function)
        if auto_range:
            self.write("R0")
        self.write(f"ITP{float(nplc):.12g}")
        self.write("SPN1")
        self.write("TRN1")
        self.write("TRS3")
        self.write("INIC0")

    def read_once(self) -> float:
        self.write("ABO")
        self.write("INI")
        self.write("*TRG")
        time.sleep(self.TRIGGER_DELAY_S)
        response = str(self.inst.read()).strip()
        match = re.search(r"[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[Ee][-+]?\d+)?", response)
        if not match:
            raise RuntimeError(f"Unexpected ADCMT 7461A measurement response: {response!r}")
        return float(match.group(0))
