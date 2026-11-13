"""Engine pipes must be UTF-8 regardless of the client locale.

The Rust engine writes UTF-8 (for example the range transition arrow "→") on
both `--default-config` and `--engine`.  On Windows, Python's locale codec is
CP932/CP1252, so inheriting it broke NativeIV().  A fake engine stands in for
iv-cli; no Rust build, device, or hardware access is involved.
"""
import json
import locale
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest

from kohdalab_iv_native import NativeIV, RUN_METADATA_SCHEMA_VERSION, SETTINGS_SCHEMA_VERSION, CSV_SCHEMA

HERE = Path(__file__).resolve().parent
ARROW = "→"
TRANSITION = f"range 1 mA {ARROW} 10 mA, 電流レンジ"
PROTOCOL = {
    "version": 1,
    "minimum_client_version": 1,
    "settings_schema_version": SETTINGS_SCHEMA_VERSION,
    "run_metadata_schema_version": RUN_METADATA_SCHEMA_VERSION,
    "csv_schema": CSV_SCHEMA,
}

# Uses raw byte streams so the fake behaves like Rust regardless of locale.
FAKE_ENGINE = textwrap.dedent("""
    import json, sys
    PROTOCOL = json.loads(%(protocol)r)
    TRANSITION = json.loads(%(transition)r)
    out = sys.stdout.buffer

    def emit(value):
        out.write(json.dumps(value, ensure_ascii=False).encode("utf-8") + b"\\n")
        out.flush()

    if sys.argv[1] == "--default-config":
        out.write(json.dumps({"range_transition": TRANSITION},
                             ensure_ascii=False).encode("utf-8"))
        out.flush()
        sys.exit(0)

    state = {"protocol": PROTOCOL, "output": "Off", "message": TRANSITION,
             "received": []}
    emit({"event": "state", "data": state})
    for raw in sys.stdin.buffer:
        command = json.loads(raw.decode("utf-8", errors="strict"))
        if command["command"] == "heartbeat":
            continue
        state["received"].append(command)
        emit({"event": "state", "data": state})
        if command["command"] == "shutdown":
            break
""")


def _write_fake_engine(directory):
    directory = Path(directory)
    script = directory / "fake_engine.py"
    # json.dumps escapes non-ASCII, so the generated files are pure ASCII.
    script.write_text(FAKE_ENGINE % {"protocol": json.dumps(PROTOCOL),
                                     "transition": json.dumps(TRANSITION)}, encoding="ascii")
    if sys.platform == "win32":
        launcher = directory / "iv-cli.cmd"
        launcher.write_text(f'@"{sys.executable}" "{script}" %*\r\n', encoding="ascii")
    else:
        launcher = directory / "iv-cli"
        launcher.write_text(f"#!/bin/sh\nexec '{sys.executable}' '{script}' \"$@\"\n",
                            encoding="utf-8")
        launcher.chmod(0o755)
    return str(launcher)


def exercise(executable):
    """Round-trip UTF-8 through both engine entry points; return locale codec."""
    with NativeIV(executable=executable) as iv:
        assert iv.default_config["range_transition"] == TRANSITION, iv.default_config
        assert iv.snapshot["message"] == TRANSITION, iv.snapshot
        for stream in (iv._process.stdin, iv._process.stdout):
            assert stream.encoding.replace("-", "").lower() == "utf8", stream.encoding
        iv._send("set_label", label=TRANSITION)
        state = iv.wait(lambda s: any(c.get("command") == "set_label" for c in s["received"]), timeout=10)
        assert state["received"][-1]["label"] == TRANSITION, state
    return locale.getpreferredencoding(False)


class EngineEncodingTests(unittest.TestCase):
    def setUp(self):
        self._directory = tempfile.TemporaryDirectory()
        self.addCleanup(self._directory.cleanup)
        self.executable = _write_fake_engine(self._directory.name)

    def test_utf8_round_trip_in_current_locale(self):
        exercise(self.executable)

    def test_utf8_round_trip_under_non_utf8_locale(self):
        env = {k: v for k, v in os.environ.items()
               if k not in ("PYTHONUTF8", "PYTHONIOENCODING") and not k.startswith("LC_")}
        env.update(LANG="C", LC_ALL="C", PYTHONCOERCECLOCALE="0")
        env["PYTHONPATH"] = os.pathsep.join(filter(None, [str(HERE), env.get("PYTHONPATH")]))
        driver = ("import sys, test_engine_encoding as t; "
                  "print(t.exercise(sys.argv[1]))")
        result = subprocess.run(
            [sys.executable, "-X", "utf8=0", "-c", driver, self.executable],
            cwd=HERE, env=env, capture_output=True, encoding="utf-8",
            errors="replace", timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        codec = result.stdout.strip().splitlines()[-1]
        if codec.replace("-", "").lower() in ("utf8", "cp65001"):
            self.skipTest(f"could not select a non-UTF-8 locale here (got {codec})")


if __name__ == "__main__":
    unittest.main()
