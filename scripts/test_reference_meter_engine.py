"""Native ``--engine`` coverage for an optional source-side reference DMM.

This test deliberately talks to the compiled Rust engine over its JSONL pipe.
The temporary helper uses the same bounded KIV framing as the production GPIB
helper, but never opens a physical device.  It exercises the complete engine
path: connect roles, configure meters, acquire, raw/CSV persistence, and the
output-OFF path after a reference-meter failure.

Example::

    python3 scripts/test_reference_meter_engine.py --binary target/debug/iv-cli
"""

from __future__ import annotations

import argparse
import csv
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest


SOURCE_ID = "YOKOGAWA,GS210,REFERENCE-FAKE,2.02"
METER_ID = "Agilent Technologies,34411A,REFERENCE-FAKE,2.40"


HELPER = r'''#!__PYTHON__
import os
import sys

SOURCE_ID = "YOKOGAWA,GS210,REFERENCE-FAKE,2.02"
METER_ID = "Agilent Technologies,34411A,REFERENCE-FAKE,2.40"
fault = os.environ.get("KIV_REFERENCE_FAULT", "")
log = os.environ.get("KIV_REFERENCE_LOG")
outputs = {3: False, 5: False}
levels = {3: 0.0, 5: 0.0}
functions = {3: "VOLT", 5: "VOLT"}
source_ranges = {3: 0.01, 5: 0.01}
source_protection = {3: 0.01, 5: 0.01}
ranges = {7: {"CURR": 0.01, "VOLT": 1.0}, 8: {"CURR": 0.01, "VOLT": 1.0}}
auto = {7: {"CURR": True, "VOLT": True}, 8: {"CURR": True, "VOLT": True}}
reference_read_after_on = False

def text(payload):
    return bytes.fromhex(payload).decode("utf-8", "replace") if payload != "-" else ""

def frame(value="", error=False):
    tag = "ERR" if error else "OK"
    sys.stdout.write("KIV\t%s\t%s\n" % (tag, value.encode().hex()))
    sys.stdout.flush()

def number(command):
    try:
        return float(command.strip().split()[-1])
    except (IndexError, ValueError):
        return 0.0

def record(operation, address, command):
    if log:
        with open(log, "a", encoding="utf-8") as stream:
            stream.write(f"{operation} {address} {command.rstrip()}\n")

frame()
for line in sys.stdin:
    fields = line.rstrip("\n").split(" ", 3)
    if len(fields) != 4:
        frame("invalid request", True)
        continue
    operation, raw_address, _timeout, payload = fields
    address = int(raw_address)
    command = text(payload)
    record(operation, address, command)
    if operation == "EXIT":
        frame()
        break
    if operation in {"LOCAL", "REN", "CLEAR", "CLAIM", "RELEASE"}:
        frame()
        continue
    if operation == "WRITE":
        if address in outputs:
            if command == ":OUTP ON\n":
                outputs[address] = True
                reference_read_after_on = False
            elif command == ":OUTP OFF\n":
                outputs[address] = False
            elif command.startswith(":SOUR:FUNC "):
                functions[address] = command.split()[-1].strip().upper()
            elif command.startswith(":SOUR:RANG "):
                source_ranges[address] = number(command)
            elif command.startswith(":SOUR:PROT:"):
                source_protection[address] = number(command)
            elif command.startswith(":SOUR:LEV"):
                levels[address] = number(command)
        elif address in ranges:
            upper = command.upper()
            quantity = "CURR" if ":CURR:" in upper else "VOLT"
            if ":RANG:AUTO ON" in upper:
                auto[address][quantity] = True
            elif ":RANG:AUTO OFF" in upper:
                auto[address][quantity] = False
            elif ":RANG " in upper:
                ranges[address][quantity] = number(command)
        frame()
        continue
    if operation not in {"QUERY", "READ"}:
        frame()
        continue
    upper = command.upper()
    if upper == "*IDN?\n":
        frame(SOURCE_ID if address in outputs else METER_ID)
    elif address in outputs and upper == ":OUTP?\n":
        frame("1" if outputs[address] else "0")
    elif address in outputs and upper == ":SOUR:FUNC?\n":
        frame(functions[address])
    elif address in outputs and upper == ":SOUR:RANG?\n":
        frame(f"{source_ranges[address]:.12e}")
    elif address in outputs and upper.startswith(":SOUR:PROT:"):
        frame(f"{source_protection[address]:.12e}")
    elif address in outputs and upper.startswith(":SOUR:LEV"):
        frame(f"{levels[address]:.12e}")
    elif address in outputs and upper.lstrip(":") == "STAT:COND?\n":
        frame("2304" if fault == "source_h_after_reference" and outputs[address] and reference_read_after_on else "256")
    elif upper == "SYST:ERR?\n":
        frame("0,No error")
    elif address in ranges and upper == "READ?\n":
        if address == 8 and (fault == "idle_overload" or (fault == "overload" and outputs[3])):
            frame("1e99")
        elif address == 8:
            reference_read_after_on = reference_read_after_on or outputs[3]
            # Intentional 90% source readback: proves source and DMM values
            # are stored independently rather than silently substituted.
            frame(f"{0.9 * levels[3]:.12e}")
        elif fault == "primary_limit" and outputs[3]:
            # This is a valid finite reading above the configured software
            # limit, not a malformed/overload transport response.
            frame("2.000000000000e-2")
        else:
            frame("1.000000000000e-6")
    elif address in ranges and ":RANG:AUTO?" in upper:
        quantity = "CURR" if ":CURR:" in upper else "VOLT"
        frame("1" if auto[address][quantity] else "0")
    elif address in ranges and ":RANG?" in upper:
        quantity = "CURR" if ":CURR:" in upper else "VOLT"
        if address == 8 and fault == "reference_range_transition" and reference_read_after_on:
            frame("1.000000000000e-1")
        else:
            frame(f"{ranges[address][quantity]:.12e}")
    elif address in ranges and ":RES?" in upper:
        frame("1e-9")
    else:
        frame("0")
'''


class Engine:
    def __init__(self, binary: Path, helper_dir: Path, work: Path, fault: str = ""):
        environment = {
            **os.environ,
            "KOHDALAB_IV_HELPER_DIR": str(helper_dir),
            # Keep the exclusive transport lease inside the disposable
            # fixture instead of the user's normal KohdaLab directory.
            "HOME": str(work),
            "USERPROFILE": str(work),
            "KIV_REFERENCE_LOG": str(work / "helper.log"),
            "KIV_REFERENCE_FAULT": fault,
        }
        # This is a direct-helper test.  A developer's optional shared hub
        # would correctly take precedence otherwise and hide the fake bridge.
        environment.pop("KOHDALAB_INSTRUMENT_HUB_CONFIG", None)
        environment.pop("KOHDALAB_INSTRUMENT_HUB_CLIENT", None)
        self.process = subprocess.Popen(
            [str(binary.resolve()), "--engine"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=environment,
        )
        self.events: queue.Queue[dict] = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self) -> None:
        assert self.process.stdout is not None
        for line in self.process.stdout:
            self.events.put(json.loads(line))

    def send(self, command: str, **fields: object) -> None:
        assert self.process.stdin is not None
        self.process.stdin.write(json.dumps({"command": command, **fields}) + "\n")
        self.process.stdin.flush()

    def wait(self, predicate, seconds: float = 10.0) -> dict:
        deadline = time.monotonic() + seconds
        next_heartbeat = 0.0
        last: dict = {}
        while time.monotonic() < deadline:
            if time.monotonic() >= next_heartbeat:
                self.send("heartbeat")
                next_heartbeat = time.monotonic() + 0.20
            try:
                event = self.events.get(timeout=0.05)
            except queue.Empty:
                if self.process.poll() is not None:
                    stderr = self.process.stderr.read() if self.process.stderr else ""
                    raise AssertionError(f"engine exited unexpectedly: {stderr}")
                continue
            if event.get("event") == "state":
                last = event["data"]
                if predicate(last):
                    return last
        raise AssertionError(f"engine state timed out; last state: {last}")

    def close(self) -> None:
        if getattr(self, "_closed", False):
            return
        if self.process.poll() is None:
            self.send("shutdown")
            assert self.process.stdin is not None
            self.process.stdin.close()
        self.process.wait(timeout=10)
        stderr = self.process.stderr.read() if self.process.stderr else ""
        if self.process.stdout is not None:
            self.process.stdout.close()
        if self.process.stderr is not None:
            self.process.stderr.close()
        self._closed = True
        if self.process.returncode:
            raise AssertionError(f"engine exit {self.process.returncode}: {stderr}")


class ReferenceMeterEngineTests(unittest.TestCase):
    binary: Path

    def setUp(self) -> None:
        self.temp = Path(tempfile.mkdtemp(prefix="iv-reference-engine-"))
        self.helper_dir = self.temp / "helpers"
        self.helper_dir.mkdir()
        helper = HELPER.replace("__PYTHON__", sys.executable)
        for name in ("kohdalab-gpib-helper", "kohdalab-usbtmc-helper"):
            path = self.helper_dir / name
            path.write_text(helper)
            path.chmod(0o700)

    def tearDown(self) -> None:
        shutil.rmtree(self.temp, ignore_errors=True)

    def engine(self, fault: str = "") -> Engine:
        result = Engine(self.binary, self.helper_dir, self.temp, fault)
        self.addCleanup(result.close)
        return result

    @staticmethod
    def config(binary: Path, mode: str, reference: bool, fixed: bool = False, dual: bool = False) -> dict:
        config = json.loads(subprocess.check_output([str(binary), "--default-config"], text=True))
        config.update({
            "mode": mode,
            "pattern": "linear",
            "start": 1e-3 if mode == "dc_iv" else 1e-6,
            "stop": 1e-3 if mode == "dc_iv" else 1e-6,
            "step": 1e-3 if mode == "dc_iv" else 1e-6,
            "source_limit": 0.01,
            "compliance": 0.01 if mode == "dc_iv" else 1.0,
            "source_compliance": 0.01 if mode == "dc_iv" else 1.0,
            "settle_s": 0.0,
            "start_settle_s": 0.0,
            "ramp_wait_s": 0.0,
            "repeat": 1,
            "average_count": 1,
            "reference_meter": {"model": "AGILENT_34411A"} if reference else None,
            "meter_voltage_range": 1.0 if fixed else None,
            "meter_current_range": 1e-3 if fixed else None,
        })
        if dual:
            config["mode"] = "dc_iv"
            config["balanced_source"] = {
                "source_model": "YOKOGAWA_GS210",
                "ratio": [1, 1],
                "source_compliance": 0.01,
            }
        return config

    @staticmethod
    def connect(engine: Engine, reference: bool = True, source2: bool = False) -> None:
        engine.send("connect_source", simulation=False, resource="GPIB0::3::INSTR", model="YOKOGAWA_GS210")
        engine.wait(lambda state: state["source_connected"])
        if source2:
            engine.send("connect_source2", simulation=False, resource="GPIB0::5::INSTR", model="YOKOGAWA_GS210")
            engine.wait(lambda state: state["source2_connected"])
        engine.send("connect_meter", simulation=False, resource="GPIB0::7::INSTR", model="AGILENT_34411A")
        engine.wait(lambda state: state["meter_connected"])
        if reference:
            engine.send("connect_reference_meter", simulation=False, resource="GPIB0::8::INSTR", model="AGILENT_34411A")
            engine.wait(lambda state: state["reference_meter_connected"])

    def run_measurement(self, engine: Engine, label: str, config: dict) -> tuple[dict, Path]:
        path = self.temp / f"{label}.csv"
        engine.send("run", config=config, path=str(path))
        state = engine.wait(lambda value: value["phase"] in {"Completed", "Failed", "Compliance stop", "Source limit stop"})
        return state, path

    @staticmethod
    def csv_row(path: Path) -> dict[str, str]:
        with path.open(newline="") as stream:
            return next(csv.DictReader(stream))

    def test_iv_reference_auto_and_absent_csv_fields(self) -> None:
        engine = self.engine()
        self.connect(engine, reference=True)
        done, path = self.run_measurement(engine, "iv-reference-auto", self.config(self.binary, "dc_iv", True))
        self.assertEqual(done["phase"], "Completed", done)
        self.assertEqual(done["output"], "Off")
        row = self.csv_row(path)
        self.assertEqual(row["V_source_V"], "1e-3")
        self.assertEqual(row["V_meter_V"], "9e-4")
        self.assertNotEqual(row["V_source_V"], row["V_meter_V"])
        self.assertEqual(row["I_source_A"], "")
        self.assertEqual(row["I_meter_A"], "1e-6")
        raw = path.with_suffix(".raw.csv").read_text()
        self.assertIn("reference_meter_reading", raw)

        engine.send("disconnect_reference_meter")
        engine.wait(lambda state: not state["reference_meter_connected"])
        done, absent = self.run_measurement(engine, "iv-reference-none", self.config(self.binary, "dc_iv", False))
        self.assertEqual(done["phase"], "Completed")
        row = self.csv_row(absent)
        self.assertEqual(row["V_source_V"], "1e-3")
        self.assertEqual(row["V_meter_V"], "")
        self.assertEqual(row["R_meter_ohm"], "")

    def test_vi_fixed_ranges_and_dual_reference(self) -> None:
        engine = self.engine()
        self.connect(engine, reference=True)
        done, vi = self.run_measurement(engine, "vi-fixed", self.config(self.binary, "dc_vi", True, fixed=True))
        self.assertEqual(done["phase"], "Completed")
        row = self.csv_row(vi)
        self.assertEqual(row["I_source_A"], "1e-6")
        self.assertEqual(row["I_meter_A"], "9e-7")
        self.assertEqual(row["V_source_V"], "")
        self.assertEqual(row["V_meter_V"], "1e-6")
        self.assertIn("SENS:VOLT:DC:RANG:AUTO OFF", (self.temp / "helper.log").read_text())

        engine.send("disconnect_reference_meter")
        engine.wait(lambda state: not state["reference_meter_connected"])
        engine.send("connect_reference_meter", simulation=False, resource="GPIB0::8::INSTR", model="AGILENT_34411A")
        engine.wait(lambda state: state["reference_meter_connected"])
        engine.send("connect_source2", simulation=False, resource="GPIB0::5::INSTR", model="YOKOGAWA_GS210")
        engine.wait(lambda state: state["source2_connected"])
        done, dual = self.run_measurement(engine, "dual-reference", self.config(self.binary, "dc_iv", True, dual=True))
        self.assertEqual(done["phase"], "Completed")
        row = self.csv_row(dual)
        self.assertTrue(row["V_meter_V"], "Dual must preserve source-side DMM data")

    def test_idle_reference_overload_stops_without_inventing_a_run(self) -> None:
        engine = self.engine("idle_overload")
        self.connect(engine, reference=True)
        state = engine.wait(lambda s: s["phase"] == "Failed", seconds=8)
        self.assertEqual(state["output"], "Off", state)
        self.assertIn("overload", state["error"].lower())
        self.assertEqual(state["path"], "")
        self.assertEqual(state["points"], 0)

    def test_reference_overload_turns_output_off_before_error(self) -> None:
        engine = self.engine("overload")
        self.connect(engine, reference=True)
        state, path = self.run_measurement(engine, "reference-overload", self.config(self.binary, "dc_iv", True))
        self.assertEqual(state["phase"], "Failed")
        self.assertEqual(state["output"], "Off")
        self.assertIn("source-quantity meter", state["error"].lower())
        self.assertIn("overload", state["error"].lower())
        commands = (self.temp / "helper.log").read_text().splitlines()
        reading = next(i for i, line in enumerate(commands) if line == "QUERY 8 READ?")
        off = next(i for i, line in enumerate(commands[reading + 1 :], reading + 1) if line == "WRITE 3 :OUTP OFF")
        self.assertGreater(off, reading)
        self.assertTrue(path.with_suffix(".raw.csv").is_file())

    def test_primary_limit_never_leaks_a_reference_read_single_or_dual(self) -> None:
        """A valid primary over-limit sample wins before the optional DMM.

        The row remains valuable evidence, but its measured source-side
        columns must stay blank: no stale reference observation may be
        attached after the primary Meter has requested protection OFF.
        """
        for dual in (False, True):
            with self.subTest(dual=dual):
                engine = self.engine("primary_limit")
                try:
                    self.connect(engine, reference=True, source2=dual)
                    state, path = self.run_measurement(
                        engine,
                        "dual-primary-limit" if dual else "primary-limit",
                        self.config(self.binary, "dc_iv", True, dual=dual),
                    )
                    self.assertEqual(state["phase"], "Compliance stop", state)
                    self.assertEqual(state["output"], "Off")
                    self.assertEqual(state["points"], 1)
                    row = self.csv_row(path)
                    self.assertEqual(row["software_limit_hit"], "true")
                    self.assertEqual(row["V_meter_V"], "")
                    self.assertEqual(row["R_meter_ohm"], "")
                    self.assertEqual(row["G_meter_S"], "")
                    commands = (self.temp / "helper.log").read_text().splitlines()
                    on = max(i for i, line in enumerate(commands) if line == "WRITE 3 :OUTP ON")
                    after_on = commands[on + 1 :]
                    self.assertFalse(
                        any(line == "QUERY 8 READ?" for line in after_on),
                        f"reference DMM read after primary limit: {after_on}",
                    )
                finally:
                    engine.close()

    def test_reference_auto_range_transition_stops_before_accepting_point(self) -> None:
        """A source-side DMM range transition is a fail-closed observation."""
        engine = self.engine("reference_range_transition")
        self.connect(engine, reference=True)
        state, path = self.run_measurement(
            engine,
            "reference-range-transition",
            self.config(self.binary, "dc_iv", True),
        )
        self.assertEqual(state["phase"], "Failed", state)
        self.assertEqual(state["output"], "Off")
        self.assertEqual(state["points"], 0)
        with path.open(newline="") as stream:
            self.assertEqual(list(csv.DictReader(stream)), [])
        commands = (self.temp / "helper.log").read_text().splitlines()
        on = max(i for i, line in enumerate(commands) if line == "WRITE 3 :OUTP ON")
        reading = next(i for i, line in enumerate(commands[on + 1 :], on + 1) if line == "QUERY 8 READ?")
        transition = next(
            i
            for i, line in enumerate(commands[reading + 1 :], reading + 1)
            if line == "QUERY 8 SENS:VOLT:DC:RANG?"
        )
        off = next(
            i
            for i, line in enumerate(commands[transition + 1 :], transition + 1)
            if line == "WRITE 3 :OUTP OFF"
        )
        self.assertGreater(off, transition)


    def test_source_h_after_reference_is_checked_before_accepting_points_or_live(self) -> None:
        for dual in (False, True):
            for settling in (False, True):
                with self.subTest(dual=dual, settling=settling):
                    engine = self.engine("source_h_after_reference")
                    try:
                        self.connect(engine, reference=True, source2=dual)
                        config = self.config(self.binary, "dc_iv", True, dual=dual)
                        config["start_settle_s"] = 0.15 if settling else 0.0
                        state, path = self.run_measurement(engine, f"post-reference-h-{dual}-{settling}", config)
                        self.assertNotEqual(state["phase"], "Completed", state)
                        self.assertEqual(state["points"], 0, state)
                        self.assertEqual(state["output"], "Off", state)
                        if dual:
                            self.assertEqual(state["source2_output"], "Off", state)
                        with path.open(newline="") as stream:
                            self.assertEqual(list(csv.DictReader(stream)), [])
                    finally:
                        engine.close()

    def test_idle_dc_checks_h_after_reference_before_publishing_new_live(self) -> None:
        engine = self.engine("source_h_after_reference")
        self.connect(engine, reference=True)
        engine.send("set_source_dc", secondary=False, mode="dc_iv", value=0.1,
                    compliance=0.01, ramp_step=0.1, range=1.0)
        state = engine.wait(lambda s: s["phase"] == "Failed", seconds=8)
        self.assertEqual(state["output"], "Off", state)
        live = state.get("live")
        self.assertTrue(live is None or live.get("reference_meter_value") != 0.09, live)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="compiled iv-cli/iv executable")
    args, unittest_args = parser.parse_known_args()
    if not args.binary.is_file():
        parser.error(f"binary does not exist: {args.binary}")
    ReferenceMeterEngineTests.binary = args.binary.resolve()
    unittest.main(argv=[sys.argv[0], *unittest_args], verbosity=2)


if __name__ == "__main__":
    main()
