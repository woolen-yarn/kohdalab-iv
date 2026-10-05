from __future__ import annotations

from kohdalab_iv.instruments.meters.keysight_34411a import Keysight34411A


class Keysight34465A(Keysight34411A):
    identity_models: tuple[str, ...] = ("34465A",)
    identity_manufacturers: tuple[str, ...] = ("KEYSIGHT", "AGILENT")

    pass
