"""Standard-library client for the Rust IV engine. No instrument driver in Python."""
import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import threading
import time


class NativeIV:
    """Own one Rust engine; use a context manager to verify safe shutdown.

    `executable` is the portable package's iv-cli(.exe). Device I/O, safety,
    CSV persistence and measurement planning remain entirely in Rust.
    """

    def __init__(self, executable=None):
        executable = executable or os.environ.get("KOHDALAB_IV_EXE") or shutil.which("iv-cli")
        if not executable:
            raise FileNotFoundError("Pass the portable package's iv-cli executable path")
        self.executable = str(Path(executable).resolve())
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

    def connect(self, source="", meter="", *, simulation=False, timeout=30):
        self._send("connect", source=source, meter=meter, simulation=simulation)
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
