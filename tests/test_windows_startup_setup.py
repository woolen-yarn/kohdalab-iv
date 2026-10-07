from __future__ import annotations

import runpy
import sys
import time
from pathlib import Path

import pytest


@pytest.fixture
def entry():
    return runpy.run_path(
        str(Path(__file__).resolve().parents[1] / "scripts/standalone_gui.py")
    )


def test_frozen_windows_setup_location(entry, tmp_path, monkeypatch):
    executable = tmp_path / "KohdaLab IV" / "KohdaLab IV.exe"
    executable.parent.mkdir()
    executable.touch()
    setup = tmp_path / "USB-Setup.exe"
    setup.touch()
    monkeypatch.setattr(entry["sys"], "executable", str(executable))
    monkeypatch.setattr(entry["sys"], "frozen", True, raising=False)
    monkeypatch.setattr(entry["sys"], "platform", "win32")
    assert entry["windows_usb_setup_path"]() == setup
    setup.unlink()
    assert entry["windows_usb_setup_path"]() is None
    monkeypatch.setattr(entry["sys"], "frozen", False)
    assert entry["windows_usb_setup_path"]() is None


@pytest.fixture(scope="module")
def qt_app():
    import os

    os.environ["QT_QPA_PLATFORM"] = "offscreen"
    from PySide6 import QtWidgets

    app = QtWidgets.QApplication.instance() or QtWidgets.QApplication([])
    yield app


@pytest.mark.parametrize("exit_code", [0, 1])
def test_async_setup_keeps_events_running_and_reports_progress(
    entry, monkeypatch, exit_code, qt_app
):
    monkeypatch.setenv("QT_QPA_PLATFORM", "offscreen")
    from PySide6 import QtCore, QtWidgets

    app = QtWidgets.QApplication.instance() or QtWidgets.QApplication([])

    class Window(QtWidgets.QWidget):
        def __init__(self):
            super().__init__()
            self.status_label = QtWidgets.QLabel(self)
            self.logs = []
            self.states = []

        def append_log(self, text):
            self.logs.append(text)

        def _sync_measurement_controls(self):
            self.states.append(self.startup_busy)

    window = Window()
    ticks = []
    timer = QtCore.QTimer()
    timer.timeout.connect(lambda: ticks.append(1))
    timer.start(10)
    start = time.monotonic()
    follow_up = []
    entry["start_usb_setup"](
        window,
        [
            sys.executable,
            "-u",
            "-c",
            f"import time; print('Checking test device', flush=True); time.sleep(0.3); raise SystemExit({exit_code})",
        ],
        on_success=lambda: follow_up.append("scan"),
    )
    assert time.monotonic() - start < 0.2
    assert window.startup_busy
    deadline = time.monotonic() + 5
    while window.startup_busy and time.monotonic() < deadline:
        app.processEvents()
        time.sleep(0.01)
    assert not window.startup_busy
    assert len(ticks) >= 5
    assert window.states == [True, False]
    assert follow_up == (["scan"] if exit_code == 0 else [])
    assert any("Checking test device" in text for text in window.logs)
    assert window.status_label.text() == (
        "idle" if exit_code == 0 else "USB setup failed"
    )
    timer.stop()
    window.close()


def test_refresh_prepares_usb_again_before_scanning(entry, monkeypatch, tmp_path):
    from types import SimpleNamespace

    setup = tmp_path / "USB-Setup.exe"
    calls = []
    scans = []
    monkeypatch.setitem(
        entry["prepare_windows_usb"].__globals__,
        "windows_usb_setup_path",
        lambda: setup,
    )
    monkeypatch.setitem(
        entry["prepare_windows_usb"].__globals__,
        "start_usb_setup",
        lambda window, command, **kwargs: calls.append((command, kwargs)),
    )
    window = SimpleNamespace(_start_resource_discovery=lambda: scans.append(1))
    entry["prepare_windows_usb"](window)
    assert calls[0][0] == [str(setup), "--ensure"]
    window.resource_prepare()
    assert calls[1][0] == [str(setup), "--ensure", "--rescan"]
    assert not scans
    calls[1][1]["on_success"]()
    assert scans == [1]


def test_missing_setup_restores_controls(entry, monkeypatch, qt_app):
    monkeypatch.setenv("QT_QPA_PLATFORM", "offscreen")
    from PySide6 import QtWidgets

    app = QtWidgets.QApplication.instance() or QtWidgets.QApplication([])
    window = QtWidgets.QWidget()
    window.status_label = QtWidgets.QLabel(window)
    window.append_log = lambda text: None
    window._sync_measurement_controls = lambda: None
    entry["start_usb_setup"](window, ["/does/not/exist/setup"])
    deadline = time.monotonic() + 3
    while window.startup_busy and time.monotonic() < deadline:
        app.processEvents()
        time.sleep(0.01)
    assert not window.startup_busy
    assert window.status_label.text() == "USB setup failed"
    window.close()


def test_compact_release_finds_setup_next_to_root_executable(
    entry, tmp_path, monkeypatch
):
    executable = tmp_path / "KohdaLab IV.exe"
    executable.touch()
    setup = tmp_path / "support" / "USB-Setup.exe"
    setup.parent.mkdir()
    setup.touch()
    monkeypatch.setattr(entry["sys"], "executable", str(executable))
    monkeypatch.setattr(entry["sys"], "frozen", True, raising=False)
    monkeypatch.setattr(entry["sys"], "platform", "win32")
    assert entry["windows_usb_setup_path"]() == setup
