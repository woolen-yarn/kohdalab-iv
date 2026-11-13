"""Reject firmware payloads and missing redistribution material in our ZIPs."""
from __future__ import annotations

import argparse
import hashlib
import io
from pathlib import Path
import tarfile
from zipfile import ZipFile

FIRMWARE_SHA256 = "2c40213a3d0d8b7d7cc083b155a5ed094e29767214a9b8aa745486f35ef58664"
LICENSES = ("GPL-2.0.txt", "GPL-3.0.txt", "LGPL-2.1.txt", "LGPL-3.0.txt")
SOURCE_FILES = (
    "native/kohdalab_gpib_helper.cpp",
    "native/agilent_82357b.cpp",
    "native/usbtmc_helper.cpp",
    "native/visa_helper.cpp",
    "native/windows_usb_setup.cpp",
    "native/vendor/ni-gpib/linux-gpib/COPYING",
    "native/vendor/libusb-1.0.30.tar.bz2",
    "native/vendor/libwdi/COPYING",
    "native/vendor/libwdi/COPYING-LGPL",
    "native/REBUILDING.md",
    "scripts/build_native_gpib.sh",
    "scripts/build_native_windows.sh",
    "scripts/build_usbtmc_probe.sh",
    "scripts/build_usb_setup.sh",
)


def check_member(name: str, data: bytes) -> None:
    lower = name.lower()
    if lower.endswith((".hex", ".ihex", ".fw")) or "setup-82357b-firmware" in lower:
        raise ValueError(f"Excluded firmware payload/downloader: {name}")
    if hashlib.sha256(data).hexdigest() == FIRMWARE_SHA256:
        raise ValueError(f"Excluded 82357B firmware content: {name}")
    # Detect the former downloader even if an obsolete binary is accidentally
    # copied under a new filename (Windows wide-string and Unix ASCII forms).
    if lower.endswith((".exe", ".dll", "kohdalab-gpib-helper")):
        for marker in ("raw.githubusercontent.com", "measat_releaseX1.8.hex"):
            if marker.encode() in data or marker.encode("utf-16le") in data:
                raise ValueError(f"Obsolete firmware-enabled binary: {name}")


def inspect_zip(data: bytes, prefix: str = "", depth: int = 0) -> set[str]:
    if depth > 3:
        raise ValueError("Unexpectedly nested source archives")
    seen = set()
    with ZipFile(io.BytesIO(data)) as archive:
        for item in archive.infolist():
            if item.is_dir():
                continue
            name = prefix + item.filename
            payload = archive.read(item)
            check_member(name, payload)
            seen.add(name)
            if item.filename.lower().endswith(".zip"):
                seen.update(inspect_zip(payload, name + "!", depth + 1))
            elif item.filename.lower().endswith(".tar.bz2"):
                with tarfile.open(fileobj=io.BytesIO(payload), mode="r:bz2") as source:
                    for member in source:
                        if member.isfile():
                            stream = source.extractfile(member)
                            assert stream is not None
                            check_member(name + "!" + member.name, stream.read())
    return seen


def inspect_tree(files: dict[str, bytes]) -> set[str]:
    """Check an already-unpacked outer payload (Linux tarball or DEB data)."""
    seen = set()
    for name, payload in files.items():
        check_member(name, payload)
        seen.add(name)
        if name.lower().endswith(".zip"):
            seen.update(inspect_zip(payload, name + "!", 1))
    return seen


def tar_members(payload: bytes) -> dict[str, bytes]:
    files = {}
    with tarfile.open(fileobj=io.BytesIO(payload), mode="r:*") as archive:
        for member in archive:
            if member.isfile():
                stream = archive.extractfile(member)
                assert stream is not None
                files[member.name.removeprefix("./")] = stream.read()
    return files


def deb_members(payload: bytes) -> dict[str, bytes]:
    """Return the files of a DEB's data archive (ar container, data.tar.*)."""
    if not payload.startswith(b"!<arch>\n"):
        raise ValueError("Not a Debian ar archive")
    offset = 8
    while offset + 60 <= len(payload):
        header = payload[offset : offset + 60]
        name = header[:16].decode().strip().rstrip("/")
        size = int(header[48:58].decode().strip())
        body = payload[offset + 60 : offset + 60 + size]
        if name.startswith("data.tar"):
            if name.endswith(".zst"):
                raise ValueError("zstd DEB data is unsupported; build with xz")
            return tar_members(body)
        offset += 60 + size + (size & 1)
    raise ValueError("DEB has no data.tar member")


def outer_files(path: Path, payload: bytes) -> dict[str, bytes]:
    name = path.name.lower()
    if name.endswith(".deb"):
        return deb_members(payload)
    if name.endswith((".tar.gz", ".tgz", ".tar.xz")):
        return tar_members(payload)
    with ZipFile(io.BytesIO(payload)) as archive:
        return {
            item.filename: archive.read(item)
            for item in archive.infolist()
            if not item.is_dir()
        }


def validate_archive(path: Path) -> None:
    payload = path.read_bytes()
    files = outer_files(path, payload)
    # Reject excluded content at every archive depth.
    if path.suffix.lower() == ".zip":
        inspect_zip(payload)
    else:
        inspect_tree(files)
    outer = set(files)
    for required in ("REDISTRIBUTION.md", "THIRD_PARTY_NOTICES.md", *LICENSES):
        if not any(name == required or name.endswith("/" + required) for name in outer):
            raise ValueError(f"Missing outer redistribution material: {required}")
    if "build-kit" in path.name:
        source = outer
    else:
        sources = [name for name in outer if name.endswith("/support/SOURCE.zip")]
        if len(sources) != 1:
            raise ValueError("Portable must carry exactly one corresponding support/SOURCE.zip")
        source = inspect_zip(files[sources[0]])
    for required in SOURCE_FILES:
        if not any(name == required or name.endswith("/" + required) for name in source):
            raise ValueError(f"Missing redistribution material: {required}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    arguments = parser.parse_args()
    validate_archive(arguments.archive)
    print(f"Redistribution material and firmware exclusion verified: {arguments.archive}")
