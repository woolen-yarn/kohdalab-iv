"""Prepare or verify the supplied Windows x64 native communication binaries."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path

from package_windows_portable import verify_pe_x64

ROOT = Path(__file__).resolve().parents[1]
NAMES = (
    "kohdalab-gpib-helper.exe",
    "kohdalab-usbtmc-helper.exe",
    "gpib-probe.exe",
    "usbtmc-probe.exe",
    "USB-Setup.exe",
    "libwdi.dll",
)


def verify(directory: Path, *, execute: bool = False) -> None:
    manifest = json.loads((directory / "SHA256.json").read_text())
    if set(manifest) != set(NAMES):
        raise RuntimeError("Native helper checksum manifest has unexpected entries")
    for name in NAMES:
        binary = directory / name
        verify_pe_x64(binary)
        if hashlib.sha256(binary.read_bytes()).hexdigest() != manifest[name]:
            raise RuntimeError(f"Native helper checksum mismatch: {name}")
    if execute:
        if sys.platform != "win32":
            raise RuntimeError("Native Windows execution checks require Windows")
        for name in ("kohdalab-usbtmc-helper.exe", "usbtmc-probe.exe", "USB-Setup.exe"):
            subprocess.run(
                [str((directory / name).resolve()), "--self-test"],
                check=True,
                timeout=15,
                capture_output=True,
                creationflags=subprocess.CREATE_NO_WINDOW,
            )
        # Checks that the GPIB executable starts and libusb enumerates USB.
        # --available does not open/reset an adapter or send GPIB/SCPI commands.
        result = subprocess.run(
            [str((directory / "kohdalab-gpib-helper.exe").resolve()), "--available"],
            check=True,
            timeout=15,
            capture_output=True,
            text=True,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        if result.stdout.strip() not in ("0", "1"):
            raise RuntimeError(
                "Native GPIB helper returned an invalid discovery result"
            )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare", action="store_true")
    parser.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    destination = ROOT / "portable/windows/native"
    if args.prepare:
        source = ROOT / "build/native-windows"
        for name in NAMES:
            folder = (
                ROOT / "build/usb-setup"
                if name in ("USB-Setup.exe", "libwdi.dll")
                else source
            )
            verify_pe_x64(folder / name)
        destination.mkdir(parents=True, exist_ok=True)
        manifest = {}
        for name in NAMES:
            folder = (
                ROOT / "build/usb-setup"
                if name in ("USB-Setup.exe", "libwdi.dll")
                else source
            )
            shutil.copy2(folder / name, destination / name)
            manifest[name] = hashlib.sha256(
                (destination / name).read_bytes()
            ).hexdigest()
        (destination / "SHA256.json").write_text(json.dumps(manifest, indent=2) + "\n")
    verify(destination, execute=args.execute)
    print("Windows native helpers: x64 headers and checksums verified.")
    if args.execute:
        print("Windows native helpers: startup and hardware-free self-tests passed.")


if __name__ == "__main__":
    main()
