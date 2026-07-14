from __future__ import annotations

import re
import time
from typing import Any

from kohdalab_iv.interfaces.common import open_visa

_FLOAT_RE = re.compile(r"[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[Ee][-+]?\d+)?")


def gpib_board_from_resource(resource: str) -> str | None:
    match = re.search(r"\b(GPIB\d*)::", str(resource), flags=re.IGNORECASE)
    if match is None:
        return None
    return match.group(1).upper()


def release_gpib_remote(board: str) -> None:
    _release_gpib_remote_pyvisa(board)
    _ni4882_set_ren(board, False)


def _release_gpib_remote_pyvisa(board: str) -> None:
    try:
        import pyvisa
        from pyvisa import constants

        resource_manager = pyvisa.ResourceManager()
    except Exception:
        return
    interface: Any = None
    try:
        interface = resource_manager.open_resource(f"{board}::INTFC")
        try:
            interface.send_command(bytes([0x3F]))
        except Exception:
            pass
        interface.control_ren(constants.VI_GPIB_REN_DEASSERT_GTL)
        interface.control_ren(constants.VI_GPIB_REN_DEASSERT)
    except Exception:
        pass
    finally:
        if interface is not None:
            try:
                interface.close()
            except Exception:
                pass
        try:
            resource_manager.close()
        except Exception:
            pass


def _ni4882_set_ren(board: str, asserted: bool, *, loader: Any | None = None) -> bool:
    import ctypes

    win_dll = loader or getattr(ctypes, "WinDLL", None)
    if win_dll is None:
        return False

    library = None
    for library_name in ("ni4882.dll", "gpib-32.dll"):
        try:
            library = win_dll(library_name)
            break
        except Exception:
            continue
    if library is None:
        return False

    ud = -1
    try:
        ibfind = getattr(library, "ibfindA", None) or getattr(library, "ibfind")
        ibfind.argtypes = [ctypes.c_char_p]
        ibfind.restype = ctypes.c_int
        ud = int(ibfind(board.lower().encode("ascii")))
        if ud < 0:
            return False

        ibsre = getattr(library, "ibsre", None)
        if ibsre is not None:
            ibsre.argtypes = [ctypes.c_int, ctypes.c_int]
            ibsre.restype = ctypes.c_int
            ibsre(ud, 1 if asserted else 0)
        else:
            ibconfig = library.ibconfig
            ibconfig.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_int]
            ibconfig.restype = ctypes.c_int
            ibconfig(ud, 0x000B, 1 if asserted else 0)
        return True
    except Exception:
        return False
    finally:
        if ud >= 0:
            try:
                ibonl = library.ibonl
                ibonl.argtypes = [ctypes.c_int, ctypes.c_int]
                ibonl.restype = ctypes.c_int
                ibonl(ud, 0)
            except Exception:
                pass


class VisaDevice:
    def __init__(
        self, resource: str, *, timeout_ms: int = 5000, handle: Any | None = None
    ):
        self.resource = resource
        self.timeout_ms = int(timeout_ms)
        self.inst: Any = (
            handle
            if handle is not None
            else open_visa(resource, timeout_ms=self.timeout_ms)
        )

    def write(self, command: str) -> None:
        self.inst.write(command)

    def query(self, command: str, *, delay_s: float = 0.0) -> str:
        if delay_s:
            self.inst.write(command)
            time.sleep(delay_s)
            return str(self.inst.read()).strip()
        return str(self.inst.query(command)).strip()

    def query_float(self, command: str, *, delay_s: float = 0.0) -> float:
        response = self.query(command, delay_s=delay_s)
        match = _FLOAT_RE.search(response)
        if not match:
            raise RuntimeError(
                f"Unexpected numeric response for {command}: {response!r}"
            )
        return float(match.group(0))

    def identify(self) -> str:
        return self.query("*IDN?")

    def clear(self) -> None:
        try:
            self.inst.clear()
        except Exception:
            pass

    def local(self) -> None:
        try:
            self.write("SYST:LOC")
        except Exception:
            pass
        self.gpib_go_to_local()

    def release_remote_control(self) -> None:
        self.gpib_interface_go_to_local(release_ren=True)
        self.gpib_send_go_to_local()
        self.gpib_go_to_local()
        self.usb_go_to_local()
        self.gpib_go_to_local(release_ren=True)
        self.usb_deassert_ren()
        self.gpib_deassert_ren()

    def gpib_go_to_local(self, *, release_ren: bool = False) -> None:
        try:
            from pyvisa import constants

            mode = (
                constants.VI_GPIB_REN_DEASSERT_GTL
                if release_ren
                else constants.VI_GPIB_REN_ADDRESS_GTL
            )
            self.inst.control_ren(mode)
        except Exception:
            pass

    def gpib_send_go_to_local(self) -> None:
        address = self._gpib_primary_address()
        if address is None:
            return
        try:
            self.inst.visalib.gpib_command(
                self.inst.session, self._gpib_gtl_command(address)
            )
        except Exception:
            pass

    def gpib_interface_go_to_local(self, *, release_ren: bool = False) -> None:
        location = self._gpib_location()
        if location is None:
            return
        board, address = location
        resource_manager = getattr(self.inst, "_resource_manager", None)
        created_resource_manager = False
        if resource_manager is None:
            try:
                import pyvisa

                resource_manager = pyvisa.ResourceManager()
                created_resource_manager = True
            except Exception:
                return
        interface: Any = None
        try:
            interface = resource_manager.open_resource(f"{board}::INTFC")
            interface.send_command(self._gpib_gtl_command(address))
            if release_ren:
                from pyvisa import constants

                interface.control_ren(constants.VI_GPIB_REN_DEASSERT_GTL)
                interface.control_ren(constants.VI_GPIB_REN_DEASSERT)
        except Exception:
            pass
        finally:
            if interface is not None:
                try:
                    interface.close()
                except Exception:
                    pass
            if created_resource_manager:
                try:
                    resource_manager.close()
                except Exception:
                    pass

    def _gpib_location(self) -> tuple[str, int] | None:
        board = gpib_board_from_resource(self.resource)
        if board is None:
            return None
        address = self._gpib_primary_address()
        if address is None:
            return None
        return board, address

    def _gpib_primary_address(self) -> int | None:
        try:
            from pyvisa import constants

            address = int(
                self.inst.get_visa_attribute(constants.VI_ATTR_GPIB_PRIMARY_ADDR)
            )
        except Exception:
            match = re.search(r"GPIB\d*::(\d+)", self.resource, flags=re.IGNORECASE)
            if match is None:
                return None
            address = int(match.group(1))
        if not 0 <= address <= 30:
            return None
        return address

    def _gpib_gtl_command(self, address: int) -> bytes:
        return bytes([0x3F, 0x20 + address, 0x01, 0x3F])

    def gpib_deassert_ren(self) -> None:
        try:
            from pyvisa import constants

            self.inst.control_ren(constants.VI_GPIB_REN_DEASSERT)
        except Exception:
            pass

    def usb_go_to_local(self) -> None:
        self._usb488_control_out(request_id=0xA1)

    def usb_deassert_ren(self) -> None:
        self._usb488_control_out(request_id=0xA0, request_value=0)

    def _usb488_control_out(self, *, request_id: int, request_value: int = 0) -> None:
        if not hasattr(self.inst, "control_out"):
            return
        try:
            from pyvisa import constants

            interface = int(
                self.inst.get_visa_attribute(constants.VI_ATTR_USB_INTFC_NUM)
            )
        except Exception:
            interface = 0
        try:
            self.inst.control_out(
                0x21, int(request_id), int(request_value), interface, b""
            )
        except Exception:
            pass

    def close(self) -> None:
        self.inst.close()

    def close_resource_manager(self) -> None:
        resource_manager = getattr(self.inst, "_resource_manager", None)
        if resource_manager is None:
            return
        try:
            resource_manager.close()
        except Exception:
            pass

    def is_connected(self) -> bool:
        if getattr(self.inst, "session", None) is None:
            return False
        if not hasattr(self.inst, "get_visa_attribute"):
            return True
        try:
            from pyvisa import constants

            self.inst.get_visa_attribute(constants.VI_ATTR_RSRC_CLASS)
        except Exception:
            return False
        return True
