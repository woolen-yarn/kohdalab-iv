from __future__ import annotations

from types import SimpleNamespace

import pytest

from kohdalab_iv.api.config import ConfigPathResolution, DEFAULT_CONFIG_PATH
from kohdalab_iv.apps import iv_gui


@pytest.fixture
def gui_window(monkeypatch, tmp_path):
    qt_widgets = pytest.importorskip("PySide6.QtWidgets")
    pytest.importorskip("pyqtgraph")

    app = qt_widgets.QApplication.instance() or qt_widgets.QApplication([])
    captured = {}
    messages = []

    monkeypatch.setattr(
        iv_gui,
        "resolve_config_path",
        lambda: ConfigPathResolution(
            path=DEFAULT_CONFIG_PATH,
            source="test",
            candidates=[],
        ),
    )
    monkeypatch.setattr(
        iv_gui, "write_last_config_path", lambda path: tmp_path / "last_config.json"
    )
    monkeypatch.setattr(iv_gui.sys, "exit", lambda code: None)
    monkeypatch.setattr(app, "exec", lambda: 0)
    monkeypatch.setattr(qt_widgets, "QApplication", lambda argv: app)
    monkeypatch.setattr(
        qt_widgets.QMainWindow,
        "show",
        lambda window: captured.setdefault("window", window),
    )
    monkeypatch.setattr(
        qt_widgets.QMessageBox,
        "critical",
        lambda parent, title, message: messages.append(
            ("critical", title, str(message))
        ),
    )
    monkeypatch.setattr(
        qt_widgets.QMessageBox,
        "warning",
        lambda parent, title, message: messages.append(
            ("warning", title, str(message))
        ),
    )

    iv_gui.main()
    window = captured["window"]
    closure = dict(
        zip(
            window.start_measurement.__func__.__code__.co_freevars,
            (
                cell.cell_contents
                for cell in window.start_measurement.__func__.__closure__
            ),
        )
    )
    window._test_messages = messages
    window._test_tmp_path = tmp_path
    window._test_qt_widgets = qt_widgets
    window._test_worker_class = closure["MeasurementWorker"]
    yield window
    window.experiment.disconnect_all()
    window.deleteLater()
    app.processEvents()


def test_gui_window_initializes_from_packaged_config(gui_window) -> None:
    window = gui_window

    assert window.windowTitle().startswith("KohdaLab IV v")
    assert window.config_path == DEFAULT_CONFIG_PATH
    assert window.config["profile"]["name"] == "default"
    assert window.start_button.isEnabled()
    assert not window.stop_button.isEnabled()
    assert "Ready." in window.log.toPlainText()


def test_gui_fields_build_valid_mode_specific_config(gui_window) -> None:
    window = gui_window
    window._set_mode("dc_iv")
    window._mode_changed()
    window.start_spin.setValue(-10.0)
    window.end_spin.setValue(20.0)
    window.step_spin.setValue(5.0)
    window.start_unit_combo.setCurrentText("mV")
    window.end_unit_combo.setCurrentText("mV")
    window.step_unit_combo.setCurrentText("mV")
    window.output_dir_edit.setText("")
    window.output_name_edit.setText("")

    config = window._config_from_fields()
    settings = config["measurements"]["iv"]

    assert window._mode() == "dc_iv"
    assert window._mode_units("dc_iv") == iv_gui.VOLTAGE_UNITS
    assert window._default_unit("dc_iv") == "mV"
    assert settings["scan"]["start"] == {"value": -10.0, "unit": "mV"}
    assert settings["scan"]["stop"] == {"value": 20.0, "unit": "mV"}
    assert settings["output"]["dir"] == "results"
    assert settings["output"]["filename"] == "iv_run"
    assert settings["safety"]["compliance"] == {"value": 10.0, "unit": "uA"}
    assert settings["safety"]["max_abs_source"] == {"value": 20.0, "unit": "mV"}


def test_gui_measurement_state_controls_and_idle_guard(gui_window) -> None:
    window = gui_window
    window.measurement_state.begin()
    window._sync_measurement_controls()

    assert not window.start_button.isEnabled()
    assert window.stop_button.isEnabled()
    assert not window.config_path_edit.isEnabled()
    assert not window._ensure_measurement_idle("Load Config")
    assert "Load Config skipped" in window.log.toPlainText()

    window.measurement_state.request_stop()
    window._sync_measurement_controls()
    assert not window.stop_button.isEnabled()

    window.handle_finished([{}, {}])
    assert window.start_button.isEnabled()
    assert window.status_label.text() == "finished 2 points"


def test_gui_plot_and_snapshot_follow_measurement_rows(gui_window) -> None:
    window = gui_window
    window.rows = [
        {
            "measured_A": 0.001,
            "measured_V": 1.0,
            "direction": "forward",
            "total_points": 3,
            "timestamp": "2026-07-13T00:00:00",
        },
        {
            "measured_A": 0.002,
            "measured_V": 2.0,
            "direction": "backward",
            "total_points": 3,
        },
        {
            "measured_A": None,
            "measured_V": 3.0,
            "direction": "custom",
            "total_points": 3,
        },
    ]

    window._set_mode("dc_vi")
    window._update_plot()
    window._update_snapshot(window.rows[0])

    assert window._plot_points() == [
        (0.001, 1.0, "forward"),
        (0.002, 2.0, "backward"),
    ]
    assert window.step_label.text() == "2/3"
    assert window.resistance_label.text() == "1 kOhm"
    assert window.conductance_label.text() in {"1 mS", "1000 uS"}
    assert window.snapshot_table.rowCount() == 3


def test_gui_reports_start_error_when_devices_are_disconnected(gui_window) -> None:
    window = gui_window

    window.start_measurement()

    assert window.measurement_state.controls_enabled
    assert window.start_button.isEnabled()
    assert any(
        kind == "critical"
        and title == "Start Error"
        and "Connect Source and Meter" in message
        for kind, title, message in window._test_messages
    )
    assert "Start error:" in window.log.toPlainText()


def test_gui_handles_points_status_errors_and_thread_cleanup(gui_window) -> None:
    window = gui_window
    window.measurement_state.begin()
    point = SimpleNamespace(
        row={
            "measured_A": 0.001,
            "measured_V": 1.0,
            "direction": "forward",
            "total_points": 1,
        }
    )

    window.handle_status("measuring 1/1")
    window.handle_point(point)
    window.handle_error("meter timeout")

    assert window.status_label.text() == "measuring 1/1"
    assert window.rows[-1] == point.row
    assert window.measurement_state.last_error == "meter timeout"
    assert window._test_messages[-1] == (
        "critical",
        "Measurement Error",
        "meter timeout",
    )

    window.worker_thread = object()
    window.worker = object()
    window._thread_finished()
    assert window.worker_thread is None
    assert window.worker is None


def test_gui_saves_loads_and_checks_config(gui_window) -> None:
    window = gui_window
    path = window._test_tmp_path / "gui-config.json"
    window.config_path_edit.setText(str(path))

    window.save_config()
    assert path.is_file()
    assert f"Saved config: {path}" in window.log.toPlainText()

    window.output_name_edit.setText("changed")
    window.load_config()
    assert window.output_name_edit.text() != "changed"
    assert f"Loaded config: {path}" in window.log.toPlainText()

    window.check_plan()
    assert "Check OK:" in window.log.toPlainText()

    window.config_path_edit.setText(str(path.with_name("missing.json")))
    window.load_config()
    assert window._test_messages[-1][0:2] == ("warning", "Config Error")


def test_gui_browse_refresh_and_panel_controls(gui_window, monkeypatch) -> None:
    window = gui_window
    qt_widgets = window._test_qt_widgets
    config_path = window._test_tmp_path / "selected.json"
    output_dir = window._test_tmp_path / "results"

    monkeypatch.setattr(
        qt_widgets.QFileDialog,
        "getOpenFileName",
        lambda *args: (str(config_path), "JSON Files (*.json)"),
    )
    monkeypatch.setattr(
        qt_widgets.QFileDialog,
        "getExistingDirectory",
        lambda *args: str(output_dir),
    )
    monkeypatch.setattr(
        iv_gui,
        "list_visa_resources",
        lambda: ("GPIB0::1::INSTR", "USB0::2::INSTR"),
    )

    window.browse_config()
    window.browse_output_dir()
    window.refresh_resources()

    assert window.config_path_edit.text() == str(config_path)
    assert window.output_dir_edit.text() == str(output_dir)
    assert window.source_resource_combo.findText("GPIB0::1::INSTR") >= 0
    assert "Resources refreshed." in window.log.toPlainText()

    window.snapshot_toggle.setChecked(False)
    window._toggle_snapshot()
    assert window.snapshot_table.isHidden()
    window.left_panel_toggle.setChecked(True)
    window._toggle_left_panel()
    assert window.left_content.isHidden()
    window.right_panel_toggle.setChecked(True)
    window._toggle_right_panel()
    assert window.right_content.isHidden()


def test_gui_connect_disconnect_and_safe_output(gui_window) -> None:
    window = gui_window

    class Device:
        def __init__(self, identity: str) -> None:
            self.identity = identity
            self.level = 1.0
            self.output = True

        def identify(self):
            return self.identity

        def output_off(self):
            self.output = False

        def set_level(self, value):
            self.level = float(value)

        def output_state(self):
            return self.output

        def read_level(self):
            return self.level

        def connect_status(self):
            return "ready"

    source = Device("SIM,SOURCE")
    meter = Device("SIM,METER")
    devices = {}

    class Session:
        def require(self, ref):
            if ref not in devices:
                raise RuntimeError("not connected")
            return devices[ref]

    class Experiment:
        config = window.config
        session = Session()

        def connect_device(self, ref):
            device = source if ref.startswith("source.") else meter
            devices[ref] = device
            return device

        def disconnect_device(self, ref):
            devices.pop(ref, None)
            return [ref]

        def disconnect_all(self):
            devices.clear()

    window.experiment = Experiment()
    window.connect_source()
    window.connect_meter()

    assert "SIM,SOURCE" in window.source_status.text()
    assert "SIM,METER" in window.meter_status.text()
    assert "ready" in window.meter_status.text()

    source.level = 2.0
    source.output = True
    window.output_off()
    assert source.level == 0.0
    assert not source.output

    window.disconnect_source()
    window.disconnect_meter()
    assert window.source_status.text() == "local"
    assert window.meter_status.text() == "local"


def test_gui_connect_status_falls_back_to_scpi(gui_window) -> None:
    window = gui_window

    class ScpiDevice:
        def write(self, command):
            assert command == "*CLS"

        def query(self, command):
            assert command == "SYST:ERR?"
            return '0,"No error"'

    class BrokenDevice:
        def write(self, command):
            raise RuntimeError("offline")

    assert window._connect_status(ScpiDevice()) == '0,"No error"'
    assert window._connect_status(BrokenDevice()) == "status unavailable: offline"


def test_measurement_worker_emits_progress_results_and_errors(gui_window) -> None:
    worker_class = gui_window._test_worker_class
    statuses = []
    points = []
    errors = []
    finished = []

    class SuccessfulExperiment:
        def run_iv(self, **kwargs):
            assert kwargs["should_continue"]()
            kwargs["on_status"]("running")
            kwargs["on_point"]("point")
            return [{"status": "ok"}]

    worker = worker_class(
        experiment=SuccessfulExperiment(),
        config=gui_window.config,
        output="result.csv",
    )
    worker.status_changed.connect(statuses.append)
    worker.point_ready.connect(points.append)
    worker.error_occurred.connect(errors.append)
    worker.finished.connect(finished.append)
    worker.run()

    assert statuses == ["running"]
    assert points == ["point"]
    assert errors == []
    assert finished == [[{"status": "ok"}]]

    worker.stop()
    assert not worker.should_continue()

    class FailingExperiment:
        def run_iv(self, **kwargs):
            raise RuntimeError("source failed")

    failed_worker = worker_class(
        experiment=FailingExperiment(),
        config=gui_window.config,
    )
    failed_errors = []
    failed_finished = []
    failed_worker.error_occurred.connect(failed_errors.append)
    failed_worker.finished.connect(failed_finished.append)
    failed_worker.run()

    assert failed_errors == ["source failed"]
    assert failed_finished == [[]]


def test_gui_model_changes_select_canonical_configs_and_compliance_units(
    gui_window,
) -> None:
    window = gui_window

    window.source_model_combo.setCurrentText("YOKOGAWA_7651")
    window._source_model_changed("YOKOGAWA_7651")
    assert (
        window.source_resource_combo.currentText()
        == window.config["instruments"]["source"]["yokogawa_7651"]["resource"]
    )

    window.meter_model_combo.setCurrentText("ADCMT_7461A")
    window._meter_model_changed("ADCMT_7461A")
    assert (
        window.meter_resource_combo.currentText()
        == window.config["instruments"]["meter"]["dmm_7461a"]["resource"]
    )

    config = window._config_from_fields()
    assert config["roles"]["iv"]["source"] == "source.yokogawa_7651"
    assert config["roles"]["iv"]["measure"] == "meter.dmm_7461a"
    assert config["instruments"]["meter"]["dmm_7461a"]["command_language"]

    safety = {"compliance": {"value": 2, "unit": "mA"}}
    window._ensure_compliance_quantity(safety, "dc_vi")
    assert safety["compliance"] == {"value": 1.0, "unit": "V"}
    safety = {"compliance": {"value": 2, "unit": "V"}}
    window._ensure_compliance_quantity(safety, "dc_iv")
    assert safety["compliance"] == {"value": 10.0, "unit": "uA"}


def test_gui_cancelled_browsing_and_preset_loading(gui_window, monkeypatch) -> None:
    window = gui_window
    qt_widgets = window._test_qt_widgets
    original_config = window.config_path_edit.text()
    original_output = window.output_dir_edit.text()

    monkeypatch.setattr(
        qt_widgets.QFileDialog, "getOpenFileName", lambda *args: ("", "")
    )
    monkeypatch.setattr(
        qt_widgets.QFileDialog, "getExistingDirectory", lambda *args: ""
    )
    window.browse_config()
    window.browse_output_dir()
    assert window.config_path_edit.text() == original_config
    assert window.output_dir_edit.text() == original_output

    path = window._test_tmp_path / "preset.json"
    window.config_path_edit.setText(str(path))
    window.save_config()
    window.output_name_edit.setText("changed")
    window.load_preset(str(path))
    assert window.output_name_edit.text() != "changed"


def test_gui_action_errors_are_reported_without_escaping(
    gui_window, monkeypatch
) -> None:
    window = gui_window

    monkeypatch.setattr(
        iv_gui,
        "save_config",
        lambda *_args: (_ for _ in ()).throw(OSError("disk full")),
    )
    window.save_config()
    assert window._test_messages[-1] == ("warning", "Config Error", "disk full")

    monkeypatch.setattr(
        iv_gui,
        "list_visa_resources",
        lambda: (_ for _ in ()).throw(RuntimeError("VISA missing")),
    )
    window.refresh_resources()
    assert window._test_messages[-1] == ("warning", "Resource Error", "VISA missing")

    monkeypatch.setattr(
        iv_gui,
        "iv_plan_from_config",
        lambda _config: (_ for _ in ()).throw(ValueError("bad plan")),
    )
    window.check_plan()
    assert window._test_messages[-1] == ("warning", "Plan Error", "bad plan")

    class BrokenExperiment:
        config = window.config

        def connect_device(self, _ref):
            raise RuntimeError("connection failed")

        def disconnect_all(self):
            pass

    window.experiment = BrokenExperiment()
    window.connect_source()
    assert window._test_messages[-1] == (
        "critical",
        "Connect Error",
        "connection failed",
    )
    window.connect_meter()
    assert window._test_messages[-1] == (
        "critical",
        "Connect Error",
        "connection failed",
    )


def test_gui_all_connect_disconnect_marking_and_idle_guards(
    gui_window, monkeypatch
) -> None:
    window = gui_window
    calls = []
    monkeypatch.setattr(window, "connect_source", lambda: calls.append("source"))
    monkeypatch.setattr(window, "connect_meter", lambda: calls.append("meter"))
    window.connect_all()
    assert calls == ["source", "meter"]

    class Experiment:
        def disconnect_all(self):
            calls.append("disconnect_all")

    window.experiment = Experiment()
    monkeypatch.setattr(window, "_safe_zero_output_off", lambda **_kwargs: None)
    window.disconnect_all()
    assert calls[-1] == "disconnect_all"
    assert window.source_status.text() == "local"
    assert window.meter_status.text() == "local"

    window._mark_disconnected(["source.a", "meter.b"])
    window.measurement_state.begin()
    window.connect_all()
    window.disconnect_all()
    window.browse_config()
    window.refresh_resources()
    window.connect_source()
    window.disconnect_source()
    window.connect_meter()
    window.disconnect_meter()
    window.output_off()
    log = window.log.toPlainText()
    assert "All Connect skipped" in log
    assert "Output Off skipped" in log


def test_gui_safe_output_handles_missing_reconnect_and_failures(gui_window) -> None:
    window = gui_window

    class Source:
        def __init__(self):
            self.level = 3.0
            self.off_calls = 0

        def output_off(self):
            self.off_calls += 1

        def set_level(self, value):
            self.level = float(value)

        def read_level(self):
            return self.level

    source = Source()

    class Session:
        def require(self, _ref):
            raise RuntimeError("not connected")

    class Experiment:
        session = Session()

        def connect_device(self, _ref):
            return source

        def disconnect_all(self):
            pass

    window.experiment = Experiment()
    assert window._safe_zero_output_off(connect_if_missing=False) is None
    assert window._safe_zero_output_off(connect_if_missing=True) is source
    assert source.level == 0.0
    assert source.off_calls == 2

    window.output_off()
    assert "Source output off and level set to zero." in window.log.toPlainText()

    class BrokenExperiment(Experiment):
        def connect_device(self, _ref):
            raise OSError("source unavailable")

    window.experiment = BrokenExperiment()
    assert (
        window._safe_zero_output_off(connect_if_missing=True, raise_errors=False)
        is None
    )
    window.output_off()
    assert window._test_messages[-1] == (
        "critical",
        "Output Off Error",
        "source unavailable",
    )


def test_gui_stop_plot_transitions_and_text_positioning(gui_window) -> None:
    window = gui_window

    class Worker:
        stopped = False

        def stop(self):
            self.stopped = True

    worker = Worker()
    window.worker = worker
    window.measurement_state.begin()
    window.stop_measurement()
    assert worker.stopped
    assert "Stop requested." in window.log.toPlainText()
    window.stop_measurement()

    window.rows = [
        {"measured_A": 1.0, "measured_V": 1.0, "direction": "forward"},
        {"measured_A": 2.0, "measured_V": 2.0, "direction": "backward"},
        {"measured_A": 3.0, "measured_V": 3.0, "direction": "forward"},
    ]
    groups = window._plot_groups()
    assert any(x != x and y != y for x, y in groups["forward"])

    positions = []

    class Text:
        def setPos(self, x, y):
            positions.append((x, y))

    window.step_text = Text()
    window.resistance_text = Text()
    window._position_plot_text()
    assert len(positions) == 2

    window.step_text = None
    window._position_plot_text()


def test_gui_starts_measurement_with_thread_signal_wiring(
    gui_window, monkeypatch
) -> None:
    window = gui_window
    closure = dict(
        zip(
            window.start_measurement.__func__.__code__.co_freevars,
            (
                cell.cell_contents
                for cell in window.start_measurement.__func__.__closure__
            ),
        )
    )
    qt_core = closure["QtCore"]
    worker_class = closure["MeasurementWorker"]

    class Signal:
        def __init__(self):
            self.callbacks = []

        def connect(self, callback):
            self.callbacks.append(callback)

        def emit(self, *args):
            for callback in list(self.callbacks):
                callback(*args)

    class Thread:
        def __init__(self):
            self.started = Signal()
            self.finished = Signal()

        def start(self):
            self.started.emit()
            self.finished.emit()

        def quit(self):
            pass

        def deleteLater(self):
            pass

    monkeypatch.setattr(qt_core, "QThread", Thread)
    monkeypatch.setattr(worker_class, "moveToThread", lambda self, thread: None)
    row = {
        "measured_A": 0.001,
        "measured_V": 1.0,
        "direction": "forward",
        "total_points": 1,
    }

    class Experiment:
        def __init__(self):
            self.config = window.config

        def connected_devices(self):
            source_ref, meter_ref = window._active_refs(self.config)
            return {source_ref: True, meter_ref: True}

        def run_iv(self, **kwargs):
            kwargs["on_status"]("running")
            kwargs["on_point"](SimpleNamespace(row=row))
            return [row]

        def disconnect_all(self):
            pass

    window.experiment = Experiment()
    window.start_measurement()

    assert window.worker_thread is None
    assert window.worker is None
    assert window.rows == [row]
    assert window.status_label.text() == "finished 1 points"
    assert "Started" in window.log.toPlainText()

    window.measurement_state.begin()
    window.start_measurement()
    assert "Start skipped: measurement is already running." in window.log.toPlainText()


def test_gui_reports_unavailable_source_and_delays_close_for_running_thread(
    gui_window, monkeypatch
) -> None:
    window = gui_window
    monkeypatch.setattr(window, "_safe_zero_output_off", lambda **_kwargs: None)
    window.output_off()
    assert window._test_messages[-1] == (
        "critical",
        "Output Off Error",
        "Source is not available.",
    )

    calls = []

    class Worker:
        def stop(self):
            calls.append("stop")

    class Thread:
        def isRunning(self):
            return True

        def quit(self):
            calls.append("quit")

        def wait(self, timeout):
            calls.append(("wait", timeout))
            return False

    class Event:
        def ignore(self):
            calls.append("ignore")

    window.worker = Worker()
    window.worker_thread = Thread()
    window.closeEvent(Event())

    assert calls == ["stop", "quit", ("wait", 15000), "ignore"]
    assert "Close delayed" in window.log.toPlainText()
    window.worker = None
    window.worker_thread = None


def test_gui_measurement_active_guards_and_normal_close(
    gui_window, monkeypatch
) -> None:
    window = gui_window
    window._set_mode("missing-mode")
    window._meter_model_changed("UNKNOWN")
    window._source_model_changed("UNKNOWN")
    safety = {"compliance": {"value": 1, "unit": "V"}}
    window._ensure_compliance_quantity(safety, "dc_vi")
    assert safety["compliance"] == {"value": 1, "unit": "V"}

    window.measurement_state.begin()
    window.load_config()
    window.save_config()
    window.connect_source()
    window.connect_meter()
    assert "Load Config skipped" in window.log.toPlainText()
    assert "Meter Connect skipped" in window.log.toPlainText()

    window.measurement_state.reset()
    calls = []
    monkeypatch.setattr(
        window, "_safe_zero_output_off", lambda **_kwargs: calls.append("safe")
    )
    monkeypatch.setattr(
        window.experiment, "disconnect_all", lambda: calls.append("disconnect")
    )
    qt_gui = pytest.importorskip("PySide6.QtGui")
    window.closeEvent(qt_gui.QCloseEvent())
    assert calls == ["safe", "disconnect"]


def test_gui_close_waits_successfully_for_running_thread(
    gui_window, monkeypatch
) -> None:
    window = gui_window
    calls = []

    class Thread:
        def isRunning(self):
            return True

        def quit(self):
            calls.append("quit")

        def wait(self, timeout):
            calls.append(("wait", timeout))
            return True

    window.worker_thread = Thread()
    monkeypatch.setattr(
        window, "_safe_zero_output_off", lambda **_kwargs: calls.append("safe")
    )
    monkeypatch.setattr(
        window.experiment, "disconnect_all", lambda: calls.append("disconnect")
    )
    qt_gui = pytest.importorskip("PySide6.QtGui")
    window.closeEvent(qt_gui.QCloseEvent())

    assert calls == ["quit", ("wait", 15000), "safe", "disconnect"]
    window.worker_thread = None


def test_gui_reports_missing_packaged_configuration(monkeypatch) -> None:
    qt_widgets = pytest.importorskip("PySide6.QtWidgets")
    pytest.importorskip("pyqtgraph")
    app = qt_widgets.QApplication.instance() or qt_widgets.QApplication([])
    monkeypatch.setattr(
        iv_gui,
        "resolve_config_path",
        lambda: ConfigPathResolution(path=None, source="none", candidates=[]),
    )
    monkeypatch.setattr(qt_widgets, "QApplication", lambda _argv: app)

    with pytest.raises(FileNotFoundError, match="packaged default configuration"):
        iv_gui.main()
