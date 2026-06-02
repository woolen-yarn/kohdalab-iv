from __future__ import annotations

import re

from kohdalab_iv.instruments.visa_base import VisaDevice


class Yokogawa7651(VisaDevice):
    def configure_source(
        self,
        *,
        source_function: str,
        source_range: float,
        hardware_compliance: float,
    ) -> None:
        self.write("H0;E")
        if source_function == "voltage":
            self.write("F1;E")
            self.write(f"R{self._range_code(source_range)};E")
            self.write(f"LA{float(hardware_compliance) * 1e3:.12g};E")
        elif source_function == "current":
            self.write("F5;E")
            self.write(f"R{self._range_code(source_range)};E")
            self.write(f"LV{float(hardware_compliance):.12g};E")
        else:
            raise ValueError(f"Unsupported 7651 source function: {source_function}")
        self.set_level(0.0)

    def identify(self) -> str:
        try:
            response = self.query("OS;E")
        except Exception:
            return "Yokogawa 7651"
        return response or "Yokogawa 7651"

    def local(self) -> None:
        self.gpib_go_to_local()

    def set_level(self, value: float) -> None:
        self.write(f"S{float(value):.12g};E")

    def read_level(self) -> float | None:
        try:
            return self.query_float("OD;E")
        except Exception:
            return None

    def output_on(self) -> None:
        self.write("O1;E")

    def output_off(self) -> None:
        self.write("O0;E")

    def output_state(self) -> bool | None:
        try:
            response = self.query("OC;E")
            match = re.search(r"[-+]?\d+\s*$", response)
            if match is None:
                return None
            return bool(int(match.group(0)) & 0b10000)
        except Exception:
            return None

    def _range_code(self, source_range: float) -> int:
        code = int(float(source_range))
        if float(source_range) != code:
            raise ValueError(f"7651 source range must be a range code, got {source_range!r}")
        return code
