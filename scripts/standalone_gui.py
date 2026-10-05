"""Entry point for the standalone desktop app."""

import os
import sys
import json
from pathlib import Path

from kohdalab_iv.api.config import config_state_dir
from kohdalab_iv.apps.iv_gui import main


def windows_usb_setup_path() -> Path | None:
    if sys.platform != "win32" or not getattr(sys, "frozen", False):
        return None
    application_dir = Path(sys.executable).resolve().parent
    for setup in (
        application_dir / "support" / "USB-Setup.exe",
        application_dir.parent / "USB-Setup.exe",  # Earlier portable layout.
    ):
        if setup.is_file():
            return setup
    return None


def start_usb_setup(window, command: list[str]) -> None:
    """Run setup after the window is visible, without blocking Qt events."""
    from PySide6 import QtCore

    process = QtCore.QProcess(window)
    timer = QtCore.QTimer(process)
    window._usb_setup_process = process
    window._usb_setup_timer = timer
    elapsed = QtCore.QElapsedTimer()
    elapsed.start()
    phase = "Checking connected USB devices"
    buffer = ""
    finished = False
    window.startup_busy = True
    window._sync_measurement_controls()
    window.append_log(
        "USB setup: checking connected devices. Windows approval may be required."
    )

    def update_status():
        seconds = elapsed.elapsed() // 1000
        window.status_label.setText(f"USB setup: {phase} ({seconds}s)")

    def read_progress():
        nonlocal phase, buffer
        buffer += bytes(process.readAllStandardOutput().data()).decode(
            "utf-8", errors="replace"
        )
        while "\n" in buffer:
            line, buffer = buffer.split("\n", 1)
            if line.strip():
                phase = line.strip()
                window.append_log(f"USB setup [{elapsed.elapsed() // 1000}s]: {phase}")
        update_status()

    def complete(code: int):
        nonlocal finished
        if finished:
            return
        finished = True
        read_progress()
        timer.stop()
        window.startup_busy = False
        window._sync_measurement_controls()
        if code == 0:
            window.status_label.setText("idle")
            window.append_log(
                f"USB setup completed in {elapsed.elapsed() // 1000}s. Click Refresh to find instruments."
            )
        else:
            window.status_label.setText("USB setup failed")
            window.append_log(
                "USB setup failed or was cancelled. Check USB-Setup.exe and usb-setup.log; the app remains open."
            )

    process.readyReadStandardOutput.connect(read_progress)
    process.finished.connect(lambda code, _status: complete(code))
    process.errorOccurred.connect(
        lambda error: (
            complete(1) if error == QtCore.QProcess.ProcessError.FailedToStart else None
        )
    )
    timer.timeout.connect(update_status)
    timer.start(1000)
    update_status()
    process.start(command[0], command[1:])


def prepare_windows_usb(window) -> None:
    setup = windows_usb_setup_path()
    if setup is not None:
        start_usb_setup(window, [str(setup), "--ensure"])


if __name__ == "__main__":
    smoke_test = len(sys.argv) == 3 and sys.argv[1] == "--smoke-test"
    if smoke_test:
        # Used only by the portable builder: start Qt, process events, then quit.
        # The build process supplies an isolated config directory and simulator.
        from PySide6 import QtCore, QtWidgets

        report_path = Path(sys.argv[2]).resolve()
        original_exec = QtWidgets.QApplication.exec

        def smoke_exec(app):
            QtCore.QTimer.singleShot(200, app.quit)
            result = original_exec()
            if result == 0:
                report_path.write_text(json.dumps({"qt_startup": "ok"}) + "\n")
            return result

        setattr(QtWidgets.QApplication, "exec", smoke_exec)
    if getattr(sys, "frozen", False):
        # Finder/Explorer may start in an unwritable directory. Keep relative
        # CSV output in the user's existing KohdaLab data directory.
        data_dir = config_state_dir()
        data_dir.mkdir(parents=True, exist_ok=True)
        os.chdir(data_dir)
    main(startup=None if smoke_test else prepare_windows_usb)
