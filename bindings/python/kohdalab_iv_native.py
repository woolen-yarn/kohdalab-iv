"""Standard-library client for the Rust IV engine. No instrument driver in Python."""
import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import time


def _cli_path(value):
    """Accept a CLI file, an installation directory, or a macOS app bundle."""
    path = Path(value).expanduser()
    if path.is_dir():
        if path.suffix.lower() == ".app":
            return path / "Contents" / "MacOS" / "iv-cli"
        return path / ("iv-cli.exe" if sys.platform == "win32" else "iv-cli")
    return path


def _existing_executable(value, source):
    path = _cli_path(value)
    if not path.is_file():
        raise FileNotFoundError(
            f"{source} points to '{path}', but iv-cli was not found there. "
            "Install KohdaLab IV first, or pass the path to iv-cli(.exe), "
            "its installation directory, or the macOS app bundle."
        )
    return str(path.resolve())


def _windows_installations():
    # Read both registry views: a 32-bit Notebook may control a 64-bit engine.
    try:
        import winreg
    except ImportError:
        return
    key = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\KohdaLabIV"
    for hive in (winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE):
        for view in (winreg.KEY_WOW64_64KEY, winreg.KEY_WOW64_32KEY):
            try:
                with winreg.OpenKey(hive, key, 0, winreg.KEY_READ | view) as handle:
                    location, kind = winreg.QueryValueEx(handle, "InstallLocation")
                if kind in (winreg.REG_SZ, winreg.REG_EXPAND_SZ) and location:
                    yield Path(os.path.expandvars(location)) / "iv-cli.exe"
            except OSError:
                continue


def _installed_candidates():
    if sys.platform == "win32":
        yield from _windows_installations()
        for variable in ("ProgramW6432", "ProgramFiles"):
            root = os.environ.get(variable, r"C:\Program Files")
            yield Path(root) / "Kohdalab" / "KohdaLab IV" / "iv-cli.exe"
    elif sys.platform == "darwin":
        for root in (Path.home() / "Applications", Path("/Applications")):
            yield root / "KohdaLab IV.app" / "Contents" / "MacOS" / "iv-cli"


def resolve_executable(executable=None):
    """Resolve explicit path, environment, PATH, then known installations.

    Discovery only inspects known locations; it never starts an engine, scans
    instruments, downloads software, or recursively searches user folders.
    An invalid explicit override raises instead of silently using another app.
    """
    if executable:
        return _existing_executable(executable, "The executable argument")
    configured = os.environ.get("KOHDALAB_IV_EXE")
    if configured:
        return _existing_executable(configured, "KOHDALAB_IV_EXE")
    for name in ("iv-cli", "iv-cli.exe"):
        from_path = shutil.which(name)
        if from_path:
            return _existing_executable(from_path, f"{name} on PATH")
    for installed in _installed_candidates():
        if installed.is_file():
            return str(installed.resolve())
    raise FileNotFoundError(
        "KohdaLab IV's Rust engine (iv-cli) was not found. Install the "
        "KohdaLab IV desktop application (on macOS, place KohdaLab IV.app "
        "in ~/Applications or /Applications), then create NativeIV(), or pass "
        "the full path with NativeIV(executable=...). Installing this Python "
        "client does not install or build the measurement engine."
    )


class NativeIV:
    """Own one Rust engine; use a context manager to verify safe shutdown.

    `executable` is the portable package's iv-cli(.exe). Device I/O, safety,
    CSV persistence and measurement planning remain entirely in Rust.
    """

    def __init__(self, executable=None):
        self.executable = resolve_executable(executable)
        self.default_config = json.loads(subprocess.check_output(
            [self.executable, "--default-config"], text=True, timeout=10))
        self._condition = threading.Condition()
        self._write_lock = threading.Lock()
        self._stop_heartbeat = threading.Event()
        self._state = {}
        self._reader_error = None
        self._eof = False
        self._closed = False
        self._source_was_connected = False
        self._process = subprocess.Popen([self.executable, "--engine"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
        self._reader = threading.Thread(target=self._read, daemon=True)
        self._reader.start()
        self._heartbeat = threading.Thread(target=self._beat, daemon=True)
        self._heartbeat.start()

    def _read(self):
        try:
            for line in self._process.stdout:
                event = json.loads(line)
                if event.get("event") == "state":
                    with self._condition:
                        self._state = event["data"]
                        self._source_was_connected |= self._state.get("source_connected", False)
                        self._condition.notify_all()
        except Exception as error:
            self._reader_error = error
        finally:
            with self._condition:
                self._eof = True
                self._condition.notify_all()

    def _beat(self):
        while not self._stop_heartbeat.wait(0.5):
            try:
                self._send("heartbeat")
            except (BrokenPipeError, OSError, RuntimeError):
                return

    def _send(self, command, **values):
        with self._write_lock:
            if self._closed:
                raise RuntimeError("Engine is closed")
            self._process.stdin.write(json.dumps({"command": command, **values}, allow_nan=False) + "\n")
            self._process.stdin.flush()

    @property
    def snapshot(self):
        with self._condition:
            return copy.deepcopy(self._state)

    def wait(self, predicate, timeout=15):
        """Wait for a snapshot predicate. Transport failures are raised."""
        deadline = time.monotonic() + timeout
        with self._condition:
            while True:
                state = copy.deepcopy(self._state)
                if state.get("error"):
                    raise RuntimeError(state["error"])
                if predicate(state):
                    return state
                if self._eof:
                    raise RuntimeError(f"Engine exited: {self._reader_error or self._process.poll()}")
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError("Rust engine response timed out")
                self._condition.wait(remaining)

    def connect(self, source="", meter="", *, simulation=False, timeout=30,
                source_model="YOKOGAWA_GS210", meter_model="AGILENT_34411A"):
        self._models = {"source_model": source_model, "meter_model": meter_model}
        self._send("connect", source=source, meter=meter, simulation=simulation, **self._models)
        return self.wait(lambda s: s.get("connected", False), timeout)

    def set_mode(self, mode, timeout=20):
        """Switch dc_iv/dc_vi while idle; Rust verifies output OFF and zero first."""
        if mode not in ("dc_iv", "dc_vi"):
            raise ValueError("mode must be dc_iv or dc_vi")
        previous = (self.snapshot.get("live") or {}).get("timestamp")
        self._send("set_live_mode", mode=mode)
        return self.wait(lambda s: s.get("output") == "Off" and s.get("live") and
            s["live"]["mode"] == mode and s["live"]["timestamp"] != previous, timeout)

    def live(self, timeout=15):
        """Return the next live sample, rather than a previously held reading."""
        previous = (self.snapshot.get("live") or {}).get("timestamp")
        return self.wait(lambda s: s.get("live") and s["live"]["timestamp"] != previous, timeout)["live"]

    def run(self, path, config=None, *, timeout=120):
        """Execute a sweep and wait for verified terminal state; save via Rust."""
        values = copy.deepcopy(self.default_config)
        values.update(getattr(self, "_models", {}))
        values.update(config or {})
        previous = self.snapshot.get("run_id", 0)
        self._send("run", path=str(Path(path).resolve()), config=values)
        try:
            return self.wait(lambda s: s.get("run_id", 0) > previous and
                s.get("phase") in ("Completed", "Stopped", "Compliance stop", "Failed"), timeout)
        except BaseException:
            # Queued stop and OFF are processed by the Rust engine even if a
            # caller timeout/interrupt prevents waiting for its response here.
            self.stop()
            self.output_off()
            raise

    def stop(self):
        self._send("stop")

    def output_off(self):
        self._send("output_off")

    def disconnect(self):
        self._send("disconnect")
        return self.wait(lambda s: not s.get("source_connected") and not s.get("meter_connected"))

    def close(self, timeout=40):
        if self._closed:
            return
        self._send("shutdown")
        self._stop_heartbeat.set()
        # Never terminate a hardware engine while output cleanup is pending.
        try:
            self._process.wait(timeout=timeout)
        except subprocess.TimeoutExpired as error:
            raise TimeoutError(f"Shutdown still pending (PID {self._process.pid}); output status unknown") from error
        self._reader.join(timeout=2)
        self._process.stdin.close()
        self._process.stdout.close()
        self._closed = True
        state = self.snapshot
        if self._process.returncode or state.get("error") or (self._source_was_connected and state.get("output") != "Off"):
            raise RuntimeError(f"Engine shutdown could not be verified: {state}")

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()
