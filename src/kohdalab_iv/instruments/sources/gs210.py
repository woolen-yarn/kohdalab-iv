from __future__ import annotations

from kohdalab_iv.instruments.visa_base import VisaDevice


class YokogawaGS210(VisaDevice):
    def configure_source(
        self,
        *,
        source_function: str,
        source_range: float,
        hardware_compliance: float,
    ) -> None:
        if source_function == "voltage":
            self.write(":SOUR:FUNC VOLT")
            self.write(f":SOUR:RANG {source_range:.12g}")
            self.write(f":SOUR:PROT:CURR {hardware_compliance:.12g}")
        elif source_function == "current":
            self.write(":SOUR:FUNC CURR")
            self.write(f":SOUR:RANG {source_range:.12g}")
            self.write(f":SOUR:PROT:VOLT {hardware_compliance:.12g}")
        else:
            raise ValueError(f"Unsupported GS210 source function: {source_function}")
        self.set_level(0.0)

    def set_level(self, value: float) -> None:
        self.write(f":SOUR:LEV:FIX {float(value):.12g}")

    def read_level(self) -> float | None:
        for command in (":SOUR:LEV?", ":SOUR:LEV:FIX?", ":SOUR:LEV:AUTO?"):
            try:
                return self.query_float(command)
            except Exception:
                continue
        return None

    def output_on(self) -> None:
        self.write(":OUTP ON")

    def output_off(self) -> None:
        self.write(":OUTP OFF")

    def output_state(self) -> bool | None:
        try:
            return bool(int(float(self.query(":OUTP?"))))
        except Exception:
            return None
