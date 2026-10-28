"""Standard-library client for the Rust IV engine. No instrument driver in Python."""
import copy
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import time
import uuid
import weakref


# Keep this contract deliberately independent of the package release number.
# It describes the JSON-lines engine protocol, not the Python distribution.
API_VERSION = 1
SETTINGS_SCHEMA_VERSION = 3
RUN_METADATA_SCHEMA_VERSION = 8
CSV_SCHEMA = "analysis_v3"
_MISSING = object()


def _protocol_integer(value, field):
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise RuntimeError(
            f"Invalid engine protocol declaration: '{field}' must be a non-negative integer."
        )
    return value


def _negotiate_protocol(declaration=_MISSING):
    """Validate the first engine state before any device command is sent.

    Engines published before protocol declarations intentionally remain v1
    compatible.  A present declaration must be complete so a newer engine is
    never used under an assumed, incompatible contract.
    """
    if declaration is _MISSING:
        return {
            "version": 1,
            "minimum_client_version": 1,
            "settings_schema_version": 0,
            "run_metadata_schema_version": RUN_METADATA_SCHEMA_VERSION,
            "csv_schema": CSV_SCHEMA,
            "legacy": True,
        }
    if not isinstance(declaration, dict):
        raise RuntimeError("Invalid engine protocol declaration: expected an object.")
    required = (
        "version",
        "minimum_client_version",
        "settings_schema_version",
        "run_metadata_schema_version",
        "csv_schema",
    )
    missing = [field for field in required if field not in declaration]
    if missing:
        raise RuntimeError(
            "Invalid engine protocol declaration: missing " + ", ".join(missing) + "."
        )
    version = _protocol_integer(declaration["version"], "version")
    minimum = _protocol_integer(
        declaration["minimum_client_version"], "minimum_client_version"
    )
    settings = _protocol_integer(
        declaration["settings_schema_version"], "settings_schema_version"
    )
    metadata = _protocol_integer(
        declaration["run_metadata_schema_version"], "run_metadata_schema_version"
    )
    csv_schema = declaration["csv_schema"]
    if not isinstance(csv_schema, str) or not csv_schema:
        raise RuntimeError(
            "Invalid engine protocol declaration: 'csv_schema' must be a non-empty string."
        )
    if version == 0 or minimum == 0 or minimum > version:
        raise RuntimeError("Invalid engine protocol declaration: incompatible version range.")
    if version > API_VERSION:
        raise RuntimeError(
            f"Engine protocol version {version} is newer than this Python client "
            f"(supports version {API_VERSION}). Upgrade kohdalab-iv-native."
        )
    if minimum > API_VERSION:
        raise RuntimeError(
            f"Engine requires Python client protocol {minimum}, but this client supports "
            f"version {API_VERSION}. Upgrade kohdalab-iv-native."
        )
    if settings > SETTINGS_SCHEMA_VERSION:
        raise RuntimeError(
            f"Engine declares settings schema {settings}, but this client supports up to "
            f"{SETTINGS_SCHEMA_VERSION}. Upgrade kohdalab-iv-native."
        )
    if metadata == 0 or metadata > RUN_METADATA_SCHEMA_VERSION:
        raise RuntimeError(
            f"Engine declares run metadata schema {metadata}, but this client supports up to "
            f"{RUN_METADATA_SCHEMA_VERSION}. Upgrade kohdalab-iv-native."
        )
    if csv_schema != CSV_SCHEMA:
        raise RuntimeError(
            f"Engine declares CSV schema '{csv_schema}', but this client supports "
            f"'{CSV_SCHEMA}'. Upgrade kohdalab-iv-native."
        )
    return {
        "version": version,
        "minimum_client_version": minimum,
        "settings_schema_version": settings,
        "run_metadata_schema_version": metadata,
        "csv_schema": csv_schema,
        "legacy": False,
    }


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
        self._received_snapshot = False
        self._reader_error = None
        self._eof = False
        self._closed = False
        self._source_was_connected = False
        self._source2_was_connected = False
        self._gate_was_connected = False
        self._process = subprocess.Popen([self.executable, "--engine"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
        self._reader = threading.Thread(target=self._read, daemon=True)
        self._reader.start()
        try:
            # Protocol negotiation uses the first normal --engine Snapshot.
            # Do not add an informational CLI flag: old engines treat unknown
            # flags as a GUI launch.
            first_snapshot = self.wait(lambda _: self._received_snapshot, timeout=15)
            self.protocol = _negotiate_protocol(first_snapshot.get("protocol", _MISSING))
        except BaseException:
            self._close_after_startup_error()
            raise
        self._heartbeat = threading.Thread(target=self._beat, daemon=True)
        self._heartbeat.start()

    def _close_after_startup_error(self):
        """Use the normal engine shutdown path before exposing construction failure."""
        try:
            self.close(timeout=10)
        except Exception:
            # No device command was sent before negotiation.  Preserve the
            # compatibility error while the existing shutdown path has made
            # its best safe attempt to stop the fresh engine.
            self._stop_heartbeat.set()

    def _read(self):
        try:
            for line in self._process.stdout:
                event = json.loads(line)
                if event.get("event") == "state":
                    with self._condition:
                        self._state = event["data"]
                        self._received_snapshot = True
                        self._source_was_connected |= self._state.get("source_connected", False)
                        self._source2_was_connected |= self._state.get("source2_connected", False)
                        self._gate_was_connected |= self._state.get("gate_connected", False)
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

    def _require_multi_source_support(self):
        if "balanced_source" not in self.default_config:
            raise RuntimeError(
                "Dual-source and gate control require a newer KohdaLab IV engine. "
                "Upgrade the application before connecting an additional source."
            )

    def connect_source2(self, resource="", *, simulation=False,
                        model="YOKOGAWA_GS210", timeout=30):
        """Connect the second balanced-IV source in this engine session."""
        self._require_multi_source_support()
        self._send("connect_source2", model=model, simulation=simulation, resource=resource)
        return self.wait(lambda s: s.get("source2_connected", False), timeout)

    def connect_gate(self, resource="", *, simulation=False,
                     model="YOKOGAWA_GS210", timeout=30):
        """Connect a fixed gate-bias source; this does not enable output."""
        self._require_multi_source_support()
        self._send("connect_gate", model=model, simulation=simulation, resource=resource)
        return self.wait(lambda s: s.get("gate_connected", False), timeout)

    def set_gate_voltage(self, voltage, *, compliance, ramp_step, timeout=30):
        """Ramp a fixed gate voltage using Rust-owned protection and cleanup."""
        self._require_multi_source_support()
        previous = self.snapshot.get("gate_revision", 0)
        self._send("set_gate_voltage", voltage=voltage, compliance=compliance,
                   ramp_step=ramp_step)
        return self.wait(lambda s: s.get("gate_revision", 0) > previous and
                         s.get("gate_output") == "On", timeout)

    def gate_output_off(self, timeout=30):
        """Turn off only the fixed gate source and verify its output state."""
        self._require_multi_source_support()
        previous = self.snapshot.get("gate_revision", 0)
        self._send("gate_output_off")
        return self.wait(lambda s: s.get("gate_revision", 0) > previous and
                         s.get("gate_output") == "Off", timeout)

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
        if values.get("source_compliance") is not None and "source_compliance" not in self.default_config:
            raise RuntimeError("Independent Source protection requires a newer KohdaLab IV engine. Upgrade the application before running this configuration.")
        if values.get("balanced_source") is not None:
            self._require_multi_source_support()
        previous = self.snapshot.get("run_id", 0)
        self._send("run", path=str(Path(path).resolve()), config=values)
        try:
            return self.wait(lambda s: s.get("run_id", 0) > previous and
                s.get("phase") in ("Completed", "Stopped", "Compliance stop", "Source limit stop", "Failed"), timeout)
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
        return self.wait(lambda s: not s.get("source_connected") and
                         not s.get("source2_connected") and
                         not s.get("gate_connected") and
                         not s.get("meter_connected"))

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
        source_unsafe = self._source_was_connected and state.get("output") != "Off"
        source2_unsafe = self._source2_was_connected and state.get("source2_output") != "Off"
        gate_unsafe = self._gate_was_connected and state.get("gate_output") != "Off"
        if self._process.returncode or state.get("error") or source_unsafe or source2_unsafe or gate_unsafe:
            raise RuntimeError(f"Engine shutdown could not be verified: {state}")

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()


class GuiIVError(RuntimeError):
    """A GUI API error with a stable, machine-readable error code."""

    def __init__(self, message, code=None):
        super().__init__(message)
        self.code = code


class _GuiControlRejected(GuiIVError):
    """The GUI explicitly rejected a request before taking an action."""


class GuiIV:
    """Attach to an already running IV GUI; never open a second device owner.

    Choose/connect instruments and set sweep conditions in the GUI first.
    External parameters stay in Python; pass their values in ``filename``.
    The context manager turns outputs off before releasing its control lease.
    """
    API_VERSION = 2
    _MUTATING_METHODS = frozenset((
        "configure", "load_config", "set_plan", "set_output", "start", "stop",
        "output_off", "connect", "disconnect", "connect_gate", "set_gate_voltage",
        "gate_output_off", "disconnect_gate",
    ))

    def __init__(self, endpoint=None):
        import socket
        self._socket = socket
        self._lease = None
        self._closed = False
        self._heartbeat_error = None
        self._control_lost = None
        self._control = None
        self._stop_heartbeat = threading.Event()
        self._heartbeat = None
        self._finalizer = weakref.finalize(self, self._stop_heartbeat.set)
        self.client_id = str(uuid.uuid4())
        if endpoint is None:
            home = Path(os.environ.get("USERPROFILE") or Path.home())
            candidates = []
            for path in (home / ".kohdalab" / "control").glob("iv-*.json"):
                try:
                    declaration = self._load_endpoint(path)
                    self._endpoint = declaration
                    self._request("status", timeout=1)
                    candidates.append((path, declaration))
                except (OSError, ValueError, RuntimeError):
                    continue
            if not candidates:
                raise RuntimeError("No running KohdaLab IV GUI found. Open the updated application first.")
            if len(candidates) > 1:
                raise RuntimeError("Several IV GUIs are open. Pass endpoint= with the desired .kohdalab/control/iv-*.json file.")
            endpoint, self._endpoint = candidates[0]
        else:
            self._endpoint = self._load_endpoint(endpoint)
        self.endpoint = Path(endpoint)
        # Construction is read-only.  Acquire a lease only when a command can
        # change a GUI-owned instrument, or when used as a context manager.

    @classmethod
    def launch(cls, profile=None, binary=None, timeout=15):
        """Launch a GUI API process and attach to its matching descriptor.

        ``profile`` is the GUI config path.  The process id in the descriptor
        prevents attachment to an older GUI that happened to be open already.
        """
        executable = resolve_executable(binary)
        command = [executable, "--gui-api"]
        if profile is not None:
            command.extend(("--config", str(Path(profile).expanduser().resolve())))
        process = subprocess.Popen(command)
        home = Path(os.environ.get("USERPROFILE") or Path.home())
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise GuiIVError("KohdaLab IV GUI exited before its API became available.", "launch_failed")
            for path in (home / ".kohdalab" / "control").glob("iv-*.json"):
                try:
                    declaration = cls._load_endpoint(path)
                except (OSError, ValueError, json.JSONDecodeError):
                    continue
                if declaration.get("pid") == process.pid:
                    candidate = cls(endpoint=path)
                    try:
                        candidate.status()
                    except BaseException:
                        candidate.close()
                        raise
                    return candidate
            time.sleep(0.1)
        raise GuiIVError("KohdaLab IV GUI API did not become available before the timeout.", "timeout")

    @staticmethod
    def _load_endpoint(path):
        import ipaddress
        try:
            declaration = json.loads(Path(path).read_text(encoding="utf-8"))
        except (OSError, UnicodeError, json.JSONDecodeError) as error:
            raise ValueError("Invalid GUI control endpoint") from error
        if not isinstance(declaration, dict):
            raise ValueError("Invalid GUI control endpoint")
        protocol = declaration.get("protocol")
        address = declaration.get("address")
        token = declaration.get("token")
        if isinstance(protocol, bool) or protocol not in (1, 2) or \
                not isinstance(address, str) or not isinstance(token, str) or not token:
            raise ValueError("Invalid GUI control endpoint")
        try:
            host, port = address.rsplit(":", 1)
            parsed_port = int(port)
            is_loopback = ipaddress.ip_address(host).is_loopback
        except (TypeError, ValueError):
            raise ValueError("Invalid GUI control endpoint") from None
        if not port.isdecimal() or not is_loopback:
            raise ValueError("Unsupported or non-local GUI control endpoint")
        if not 0 < parsed_port < 65536:
            raise ValueError("Invalid GUI control endpoint")
        if protocol == 2:
            pid = declaration.get("pid")
            instance_id = declaration.get("instance_id")
            if isinstance(pid, bool) or not isinstance(pid, int) or pid <= 0 or \
                    not isinstance(instance_id, str) or not instance_id:
                raise ValueError("Invalid GUI control endpoint")
        return declaration

    def _request(self, method, timeout=12, require_lease=False, **values):
        if self._closed:
            raise GuiIVError("GUI control is closed", "closed")
        if require_lease and self._control_lost is not None:
            raise self._control_lost
        if require_lease:
            self._ensure_lease()
        host, port = self._endpoint["address"].rsplit(":", 1)
        payload = {"token": self._endpoint["token"], "client_id": self.client_id,
                   "method": method, "action": method, **values}
        if self._lease is not None:
            payload["lease"] = self._lease
        payload = json.dumps(payload, allow_nan=False).encode() + b"\n"
        with self._socket.create_connection((host, int(port)), timeout=timeout) as connection:
            connection.settimeout(timeout)
            connection.sendall(payload)
            with connection.makefile("rb") as stream:
                line = stream.readline(2_097_153)
        if len(line) > 2_097_152 or not line.endswith(b"\n"):
            raise GuiIVError("Invalid GUI control response", "protocol")
        try:
            response = json.loads(line)
        except (TypeError, json.JSONDecodeError) as error:
            raise GuiIVError("Invalid GUI control response", "protocol") from error
        if not isinstance(response, dict) or not isinstance(response.get("ok"), bool):
            raise GuiIVError("Invalid GUI control response", "protocol")
        if response["ok"] and "data" not in response and "result" not in response:
            raise GuiIVError("Invalid GUI control response", "protocol")
        if not response.get("ok"):
            error = _GuiControlRejected(response.get("error", "GUI control failed"),
                                        response.get("error_code", "rejected"))
            # A revoked or expired lease must never cause a later safety call
            # to acquire a different client's GUI session.
            if self._lease is not None and error.code in (
                "lease_expired", "lease_revoked", "not_owner", "ownership_lost",
                "invalid_lease", "heartbeat_failed",
            ):
                self._control_lost = error
            raise error
        return response["data"] if "data" in response else response.get("result")

    def _ensure_lease(self):
        if self._control_lost is not None:
            raise self._control_lost
        if self._lease is not None:
            return self._lease
        method = "claim" if self._endpoint.get("protocol") == 1 else "acquire"
        result = self._request(method, timeout=12)
        lease = result.get("lease") if isinstance(result, dict) else None
        if not isinstance(lease, str) or not lease:
            raise GuiIVError("GUI returned an invalid control lease", "protocol")
        self._lease = lease
        self._control = result.get("control", {"owned": True, "client_id": self.client_id})
        self._start_heartbeat()
        return lease

    def _start_heartbeat(self):
        if self._stop_heartbeat.is_set():
            self._stop_heartbeat = threading.Event()
            self._finalizer.detach()
            self._finalizer = weakref.finalize(self, self._stop_heartbeat.set)
            self._heartbeat = None
        if self._heartbeat is not None:
            return
        reference = weakref.ref(self)
        self._heartbeat = threading.Thread(
            target=GuiIV._beat_gui_weak,
            args=(reference, self._stop_heartbeat), daemon=True,
        )
        self._heartbeat.start()

    @staticmethod
    def _beat_gui_weak(reference, stop_event):
        while not stop_event.wait(1):
            client = reference()
            if client is None:
                return
            try:
                client._request("heartbeat", timeout=3, require_lease=False)
            except Exception as error:
                if isinstance(error, OSError):
                    error = GuiIVError("GUI control heartbeat connection was lost.", "ownership_lost")
                client._heartbeat_error = error
                if isinstance(error, GuiIVError):
                    client._control_lost = error
                return
            finally:
                del client

    @property
    def snapshot(self):
        data = self._request("status")
        return data.get("snapshot", data) if isinstance(data, dict) else data

    @property
    def configuration(self):
        """Legacy property returning the GUI's current Rust RunConfig."""
        return self.plan()

    def capabilities(self):
        """Return the GUI API version, actions, and its safety policy."""
        return self._request("capabilities")

    def status(self):
        """Return the complete GUI status envelope without acquiring control."""
        return self._request("status")

    def config(self):
        """Return the full saved GUI settings and selected config path."""
        try:
            return self._request("config")
        except _GuiControlRejected:
            if self._endpoint.get("protocol") == 1:
                return self._request("configuration")
            raise

    def plan(self):
        """Return the current Rust RunConfig without acquiring control."""
        return self._request("configuration" if self._endpoint.get("protocol") == 1 else "plan")

    def output(self):
        """Return the GUI's output directory, filename and suffix policy."""
        return self._request("output")

    def acquire(self, label=None):
        """Acquire exclusive Python control while the GUI is idle."""
        if label is None:
            self._ensure_lease()
            return self._control
        if self._lease is not None:
            return self._control
        if self._control_lost is not None:
            raise self._control_lost
        method = "claim" if self._endpoint.get("protocol") == 1 else "acquire"
        result = self._request(method, label=str(label))
        lease = result.get("lease") if isinstance(result, dict) else None
        if not isinstance(lease, str) or not lease:
            raise GuiIVError("GUI returned an invalid control lease", "protocol")
        self._lease = lease
        self._control = result.get("control", result)
        self._start_heartbeat()
        return self._control

    def heartbeat(self):
        """Send an explicit lease heartbeat (normally automatic)."""
        return self._request("heartbeat", require_lease=True)

    def release(self, timeout=40):
        """Safely turn outputs off, then relinquish Python control."""
        if self._lease is None:
            return
        self._safe_output_off(timeout)
        self._request("release", require_lease=False)
        self._lease = None
        self._control = None
        self._stop_heartbeat.set()

    def configure(self, profile=None):
        return self._request("configure", require_lease=True, profile=profile)

    def load_config(self, path):
        return self._request("load_config", require_lease=True, path=str(Path(path).expanduser().resolve()))

    def set_plan(self, plan):
        result = self._request("set_plan", require_lease=True, plan=copy.deepcopy(plan))
        if self._is_job(result):
            self.wait(result["id"])
            return self.config()
        return result

    def set_output(self, directory=None, filename=None, auto_timestamp_suffix=None):
        values = {}
        if directory is not None:
            values["directory"] = str(Path(directory).expanduser().resolve())
        if filename is not None:
            values["filename"] = filename
        if auto_timestamp_suffix is not None:
            values["auto_timestamp_suffix"] = auto_timestamp_suffix
        return self._request("set_output", require_lease=True, **values)

    @staticmethod
    def _is_job(value):
        return isinstance(value, dict) and isinstance(value.get("id"), str)

    def wait(self, run_id=None, timeout=30, predicate=None, poll_interval=0.1):
        """Wait for a job receipt, or retain legacy ``wait(predicate)`` polling."""
        if callable(run_id):
            predicate = run_id
            run_id = None
        if timeout is not None and (not math.isfinite(timeout) or timeout < 0):
            raise ValueError("timeout must be a non-negative finite number or None")
        if not math.isfinite(poll_interval) or poll_interval <= 0:
            raise ValueError("poll_interval must be positive and finite")
        deadline = None if timeout is None else time.monotonic() + timeout
        while True:
            if self._heartbeat_error:
                raise GuiIVError(f"GUI control heartbeat failed: {self._heartbeat_error}",
                                 "heartbeat_failed")
            if run_id is not None:
                receipt = self.result(run_id)
                if receipt.get("status") in ("completed", "stopped", "error"):
                    if receipt.get("status") in ("stopped", "error"):
                        raise GuiIVError(receipt.get("error", "GUI job failed"),
                                         receipt.get("error_code", receipt.get("status")))
                    return receipt
            else:
                state = self.snapshot
                if state.get("error"):
                    raise GuiIVError(state["error"], state.get("error_code"))
                if predicate is None or predicate(state):
                    return state
            if deadline is not None and time.monotonic() >= deadline:
                raise GuiIVError("GUI response timed out. Inspect the GUI before retrying.", "timeout")
            time.sleep(poll_interval)

    def start(self, filename=None, *, directory=None, request_id=None):
        """Start a GUI run and return its idempotent job receipt."""
        request_id = self._validated_request_id(request_id)
        return self._request("start", require_lease=True, filename=filename,
                             directory=str(Path(directory).expanduser().resolve()) if directory is not None else None,
                             request_id=request_id)

    @staticmethod
    def _validated_request_id(request_id):
        if request_id is None:
            return str(uuid.uuid4())
        if not isinstance(request_id, str):
            raise GuiIVError("request_id must be a UUID string.", "invalid_request_id")
        try:
            return str(uuid.UUID(request_id))
        except (AttributeError, ValueError) as error:
            raise GuiIVError("request_id must be a UUID string.", "invalid_request_id") from error

    def result(self, run_id):
        """Read a job receipt without taking control of the GUI."""
        return self._request("result", run_id=run_id)

    def run(self, filename=None, *, directory=None, timeout=120):
        """Start the GUI's configured sweep and wait. GUI plots/CSV update normally."""
        try:
            accepted = self.start(filename=filename, directory=directory)
        except _GuiControlRejected:
            raise  # e.g. a manual run is busy: do not interrupt it.
        except BaseException as original:
            self._abort_after_error(original)
            raise
        try:
            target = accepted.get("id", accepted.get("run_id"))
            if target is None:
                raise GuiIVError("GUI returned no run id", "protocol")
            # Protocol v2 returns a durable receipt; protocol v1 used state polling.
            if "id" in accepted:
                receipt = self.wait(target, timeout)
                return receipt["snapshot"] if "snapshot" in receipt else self.snapshot
            safety = accepted.get("safety_revision", 0)
            state = self.wait(lambda s: (s.get("run_id", 0) >= target and s.get("phase") in
                              ("Completed", "Stopped", "Compliance stop", "Source limit stop", "Failed")) or
                              s.get("safety_revision", 0) > safety, timeout)
            if state.get("run_id", 0) < target:
                raise RuntimeError("GUI measurement was cancelled before it started.")
            if state["phase"] != "Completed":
                raise RuntimeError(f"GUI measurement ended with {state['phase']}: {state.get('message', '')}")
            return state
        except BaseException as original:
            self._abort_after_error(original)
            raise

    def _abort_after_error(self, original):
        failures = []
        for operation in (self.stop, self.output_off):
            try:
                operation()
            except Exception as failure:
                failures.append(str(failure))
        if failures:
            raise GuiIVError(f"{original}. Safety cleanup could not be confirmed: {'; '.join(failures)}. Check instrument outputs manually.",
                             "cleanup_unverified") from original

    def _job_snapshot_or_legacy_wait(self, result, predicate, timeout):
        if getattr(self, "_endpoint", {}).get("protocol") == 2 and self._is_job(result):
            receipt = self.wait(result["id"], timeout)
            return receipt["snapshot"] if "snapshot" in receipt else self.snapshot
        return self.wait(predicate, timeout)

    def connect_gate(self, resource="", *, simulation=False, model="YOKOGAWA_GS210", timeout=30):
        if self.snapshot.get("gate_connected"):
            raise GuiIVError("Gate is already connected. Disconnect it before changing its binding.", "busy")
        result = self._request("connect_gate", require_lease=True, resource=resource, simulation=simulation, model=model)
        return self._job_snapshot_or_legacy_wait(result, lambda s: s.get("gate_connected"), timeout)

    def set_gate_voltage(self, voltage, *, compliance, ramp_step=0.001, timeout=30):
        try:
            previous = self.snapshot.get("gate_revision", 0)
            result = self._request("set_gate_voltage", require_lease=True, voltage=voltage, compliance=compliance, ramp_step=ramp_step)
        except _GuiControlRejected:
            raise
        except BaseException as original:
            self._abort_after_error(original)
            raise
        try:
            return self._job_snapshot_or_legacy_wait(
                result, lambda s: s.get("gate_revision", 0) > previous and s.get("gate_output") == "On", timeout)
        except BaseException as original:
            self._abort_after_error(original)
            raise

    def gate_output_off(self, timeout=30):
        self._request("gate_output_off", require_lease=True)
        deadline = time.monotonic() + timeout
        while True:
            state = self.snapshot
            # A protection message does not invalidate an already accepted
            # direct safety command; wait for its physical acknowledgement.
            if state.get("gate_output") == "Off":
                return state
            if time.monotonic() >= deadline:
                raise GuiIVError("Gate Output OFF could not be verified. Check the gate output manually.",
                                 "timeout")
            time.sleep(0.1)

    def disconnect_gate(self, timeout=30):
        result = self._request("disconnect_gate", require_lease=True)
        return self._job_snapshot_or_legacy_wait(result, lambda s: not s.get("gate_connected"), timeout)

    def connect(self, devices=None, *, request_id=None):
        """Connect selected devices; omit devices to connect active roles."""
        values = {"request_id": self._validated_request_id(request_id)}
        if devices is not None:
            values["devices"] = list(devices)
        return self._request("connect", require_lease=True, **values)

    def disconnect(self, devices=None, *, request_id=None):
        """Disconnect selected devices; omit devices to disconnect all."""
        values = {"request_id": self._validated_request_id(request_id)}
        if devices is not None:
            values["devices"] = list(devices)
        return self._request("disconnect", require_lease=True, **values)

    def stop(self, run_id=None):
        values = {} if run_id is None else {"run_id": run_id}
        return self._request("stop", require_lease=True, **values)

    def output_off(self):
        return self._request("output_off", require_lease=True)

    def _safe_output_off(self, timeout):
        self.output_off()
        deadline = time.monotonic() + timeout
        while True:
            state = self.snapshot
            off = (not state.get("source_connected") or state.get("output") == "Off") and \
                  (not state.get("source2_connected") or state.get("source2_output") == "Off") and \
                  (not state.get("gate_connected") or state.get("gate_output") == "Off")
            if off and state.get("phase") not in ("Preparing", "Running", "Cleaning up"):
                return state
            if time.monotonic() >= deadline:
                raise GuiIVError("Output OFF could not be verified. Check all instrument outputs manually.",
                                 "timeout")
            time.sleep(0.1)

    def close(self, timeout=40):
        if self._closed:
            return
        error = None
        try:
            if self._lease is not None:
                self.release(timeout)
        except BaseException as failure:
            error = failure
        finally:
            self._stop_heartbeat.set()
            if self._heartbeat is not None:
                self._heartbeat.join(timeout=4)
            self._closed = True
        if error:
            raise error

    def __enter__(self):
        self.acquire()
        return self

    def __exit__(self, *exc):
        self.close()
