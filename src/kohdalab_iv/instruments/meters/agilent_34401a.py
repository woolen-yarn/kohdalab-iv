from __future__ import annotations

from kohdalab_iv.instruments.meters.scpi_dmm import ScpiDMM


class Agilent34401A(ScpiDMM):
    identity_models: tuple[str, ...] = ("34401A",)
    identity_manufacturers: tuple[str, ...] = ("HEWLETT-PACKARD", "AGILENT", "KEYSIGHT")

    pass
