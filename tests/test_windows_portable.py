from __future__ import annotations

import csv
import hashlib
import json
import runpy
import shutil
import struct
from pathlib import Path

import pytest

TOOLS = runpy.run_path(
    str(Path(__file__).resolve().parents[1] / "scripts/package_windows_portable.py")
)


def test_windows_executable_architecture_and_invalid_headers(tmp_path):
    path = tmp_path / "app.exe"
    data = bytearray(128)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 64)
    data[64:68] = b"PE\0\0"
    struct.pack_into("<H", data, 68, 0x8664)
    struct.pack_into("<H", data, 88, 0x20B)
    path.write_bytes(data)
    TOOLS["verify_pe_x64"](path)
    struct.pack_into("<H", data, 68, 0x14C)  # 32-bit Windows.
    path.write_bytes(data)
    with pytest.raises(RuntimeError, match="x64"):
        TOOLS["verify_pe_x64"](path)
    struct.pack_into("<I", data, 0x3C, 2**32 - 1)
    path.write_bytes(data)
    with pytest.raises(RuntimeError, match="PE header"):
        TOOLS["verify_pe_x64"](path)
    path.write_bytes(b"MZ")
    with pytest.raises(RuntimeError, match="Not a Windows"):
        TOOLS["verify_pe_x64"](path)


def test_frozen_sweep_validation_rejects_wrong_or_missing_results(tmp_path):
    path = tmp_path / "measurement.csv"

    def write_rows(currents):
        with path.open("w", newline="") as stream:
            writer = csv.writer(stream)
            writer.writerow(["voltage_V", "current_A"])
            writer.writerows(zip((-0.01, 0.0, 0.01), currents))

    write_rows((-1e-5, 0.0, 1e-5))
    TOOLS["verify_simulated_csv"](path)
    write_rows((-1e-5, 0.0, 1e-3))
    with pytest.raises(RuntimeError, match="current"):
        TOOLS["verify_simulated_csv"](path)
    write_rows((-1e-5,))
    with pytest.raises(RuntimeError, match="three points"):
        TOOLS["verify_simulated_csv"](path)


def test_native_helper_validation_rejects_altered_binary_and_manifest(
    tmp_path, monkeypatch
):
    root = Path(__file__).resolve().parents[1]
    monkeypatch.syspath_prepend(str(root / "scripts"))
    native = runpy.run_path(str(root / "scripts/windows_native.py"))
    for name in native["NAMES"]:
        shutil.copy2(root / "portable/windows/native" / name, tmp_path / name)
    manifest = {
        name: hashlib.sha256((tmp_path / name).read_bytes()).hexdigest()
        for name in native["NAMES"]
    }
    checksum = tmp_path / "SHA256.json"
    checksum.write_text(json.dumps(manifest))
    native["verify"](tmp_path)
    binary = tmp_path / native["NAMES"][0]
    with binary.open("ab") as stream:
        stream.write(b"tampered")
    with pytest.raises(RuntimeError, match="checksum mismatch"):
        native["verify"](tmp_path)
    checksum.write_text(json.dumps({"../unexpected.exe": "00"}))
    with pytest.raises(RuntimeError, match="unexpected entries"):
        native["verify"](tmp_path)


def test_compact_release_layout_preserves_runtime_setup_and_source(tmp_path):
    import zipfile

    gui = tmp_path / "build" / "GUI"
    (gui / "_internal").mkdir(parents=True)
    (gui / "KohdaLab IV.exe").write_bytes(b"GUI")
    for name in (
        "runtime.dll",
        "kohdalab-gpib-helper.exe",
        "kohdalab-usbtmc-helper.exe",
    ):
        (gui / "_internal" / name).write_bytes(b"Bundled component")
    (gui / ".DS_Store").touch()
    package = tmp_path / "release" / "portable"
    support = TOOLS["assemble_layout"](package, gui)
    assert (package / "_internal/runtime.dll").read_bytes() == b"Bundled component"
    assert (support / "libwdi.dll").is_file()
    assert (support / "licenses/LICENSE-MIT.txt").is_file()
    assert (support / "diagnostics/gpib-probe.exe").is_file()
    assert (support / "configs/simulated.json").is_file()
    assert b"\\r\\n" not in (support / "Setup-Instrument-Drivers.cmd").read_bytes()
    source = tmp_path / "source" / "application"
    (source / "native").mkdir(parents=True)
    (source / "native/driver.cpp").write_text("// corresponding source")
    (source / "LICENSE").write_text("MIT license")
    TOOLS["archive_corresponding_source"](source, support / "SOURCE.zip")
    with zipfile.ZipFile(support / "SOURCE.zip") as archive:
        assert (
            archive.read("application/native/driver.cpp") == b"// corresponding source"
        )
        assert archive.read("application/LICENSE") == b"MIT license"
    (package / "README.txt").write_text("Open KohdaLab IV.exe")
    (support / "BUILD_INFO.json").write_text("{}")
    TOOLS["verify_release_layout"](package)
    assert {p.name for p in package.iterdir()} == {
        "KohdaLab IV.exe",
        "README.txt",
        "_internal",
        "support",
    }
    (package / "unexpected.txt").touch()
    with pytest.raises(RuntimeError, match="top-level"):
        TOOLS["verify_release_layout"](package)
    (package / "unexpected.txt").unlink()
    (support / "USB-Setup.exe").unlink()
    with pytest.raises(RuntimeError, match="USB-Setup.exe"):
        TOOLS["verify_release_layout"](package)


def test_release_build_launcher_has_literal_windows_paths_and_no_control_characters():
    root = Path(__file__).resolve().parents[1]
    builder = runpy.run_path(str(root / "scripts/create_windows_release_build_kit.py"))
    command = builder["launcher"]("0.2.3")
    assert r".\scripts\build_windows_portable.ps1" in command
    assert r"project\dist\releases\v0.2.3\windows-x64" in command
    assert "Release" in command
    assert all(ord(c) >= 32 or c in "\r\n" for c in command)
