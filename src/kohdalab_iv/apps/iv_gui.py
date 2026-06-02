from __future__ import annotations

import copy
import sys
from pathlib import Path
from typing import Any

from kohdalab_iv.api.config import load_config, output_path, resolve_config_path, save_config, write_last_config_path
from kohdalab_iv.api.experiment import Experiment
from kohdalab_iv.api.formatting import format_conductance, format_resistance
from kohdalab_iv.api.scan_plan import iv_plan_from_config
from kohdalab_iv.interfaces.common import list_visa_resources


SOURCE_MODELS = ["YOKOGAWA_GS210"]
METER_MODELS = ["AGILENT_34401A", "AGILENT_34411A", "KEYSIGHT_34411A", "KEYSIGHT_34465A", "ADCMT_7461A"]
CURRENT_UNITS = ["pA", "nA", "uA", "mA", "A"]
VOLTAGE_UNITS = ["nV", "uV", "mV", "V"]
MODE_LABELS = {
    "dc_vi": "I-V (source I, measure V)",
    "dc_iv": "V-I (source V, measure I)",
}
SNAPSHOT_FIELDS = [
    "timestamp",
    "elapsed_s",
    "point_index",
    "total_points",
    "direction",
    "source_target_value",
    "source_target_unit",
    "source_readback_value",
    "source_readback_unit",
    "voltage_V",
    "voltage_origin",
    "current_A",
    "current_origin",
    "resistance_Ohm",
    "conductance_S",
]


def _set_quantity(target: dict[str, Any], key: str, value: float, unit: str) -> None:
    target[key] = {"value": float(value), "unit": unit}


def _quantity_value(data: dict[str, Any], key: str, default_value: float, default_unit: str) -> tuple[float, str]:
    value = data.get(key, {})
    if isinstance(value, dict):
        return float(value.get("value", default_value)), str(value.get("unit", default_unit))
    return float(value), default_unit


def _short(value: Any, unit: str = "") -> str:
    if value is None:
        return "-"
    if isinstance(value, float):
        suffix = f" {unit}" if unit else ""
        return f"{value:.6g}{suffix}"
    return str(value)


def _unit_family(unit: str) -> str | None:
    key = str(unit).strip()
    if key in CURRENT_UNITS:
        return "current"
    if key in VOLTAGE_UNITS:
        return "voltage"
    return None


def _resistance_from_rows(rows: list[dict[str, Any]]) -> float | None:
    pairs = [
        (float(row["measured_A"]), float(row["measured_V"]))
        for row in rows
        if row.get("measured_A") is not None and row.get("measured_V") is not None
    ]
    if len(pairs) < 2:
        return None
    mean_i = sum(i for i, _ in pairs) / len(pairs)
    mean_v = sum(v for _, v in pairs) / len(pairs)
    denominator = sum((i - mean_i) ** 2 for i, _ in pairs)
    if denominator == 0:
        return None
    return sum((i - mean_i) * (v - mean_v) for i, v in pairs) / denominator


def main() -> None:
    from PySide6 import QtCore, QtWidgets
    import pyqtgraph as pg

    pg.setConfigOption("background", "#050505")
    pg.setConfigOption("foreground", "#e8e8e8")

    class MeasurementWorker(QtCore.QObject):
        point_ready = QtCore.Signal(object)
        status_changed = QtCore.Signal(str)
        error_occurred = QtCore.Signal(str)
        finished = QtCore.Signal(object)

        def __init__(self, *, experiment: Experiment, config: dict[str, Any], output: str | None = None):
            super().__init__()
            self.experiment = experiment
            self.config = config
            self.output = output
            self._running = True

        def stop(self) -> None:
            self._running = False

        def should_continue(self) -> bool:
            return self._running

        @QtCore.Slot()
        def run(self) -> None:
            rows = []
            try:
                plan = iv_plan_from_config(self.config)
                rows = self.experiment.run_iv(
                    plan=plan,
                    output=self.output,
                    on_point=self.point_ready.emit,
                    on_status=self.status_changed.emit,
                    should_continue=self.should_continue,
                )
            except Exception as e:
                self.error_occurred.emit(str(e))
            finally:
                self.finished.emit(rows)

    class IVGui(QtWidgets.QMainWindow):
        def __init__(self):
            super().__init__()
            self.setWindowTitle("KohdaLab IV")
            self._apply_dark_theme()
            config_resolution = resolve_config_path()
            self.config_path = config_resolution.path or Path("config/default.json")
            self.config = load_config(self.config_path)
            if config_resolution.path is not None:
                write_last_config_path(config_resolution.path)
            self.experiment = Experiment(self.config, auto_connect=False)
            self.thread: QtCore.QThread | None = None
            self.worker: MeasurementWorker | None = None
            self.rows: list[dict[str, Any]] = []
            self._build_widgets()
            self._build_layout()
            self._load_fields()
            self.append_log("Ready.")

        def _build_widgets(self) -> None:
            self.config_path_edit = QtWidgets.QLineEdit(str(self.config_path))
            self.browse_button = QtWidgets.QPushButton("Browse")
            self.load_button = QtWidgets.QPushButton("Load")
            self.save_button = QtWidgets.QPushButton("Save")
            self.connect_all_button = QtWidgets.QPushButton("All Connect")
            self.disconnect_all_button = QtWidgets.QPushButton("All Disconnect")
            self.browse_button.clicked.connect(self.browse_config)
            self.load_button.clicked.connect(self.load_config)
            self.save_button.clicked.connect(self.save_config)
            self.connect_all_button.clicked.connect(self.connect_all)
            self.disconnect_all_button.clicked.connect(self.disconnect_all)

            self.source_model_combo = QtWidgets.QComboBox()
            self.source_model_combo.addItems(SOURCE_MODELS)
            self.source_resource_combo = QtWidgets.QComboBox()
            self.source_resource_combo.setEditable(True)
            self.source_refresh_button = QtWidgets.QPushButton("Refresh")
            self.source_connect_button = QtWidgets.QPushButton("Connect")
            self.source_disconnect_button = QtWidgets.QPushButton("Disconnect")
            self.source_status = QtWidgets.QLabel("-")
            self.source_status.setWordWrap(True)
            self.source_refresh_button.clicked.connect(self.refresh_resources)
            self.source_connect_button.clicked.connect(self.connect_source)
            self.source_disconnect_button.clicked.connect(self.disconnect_source)

            self.meter_model_combo = QtWidgets.QComboBox()
            self.meter_model_combo.addItems(METER_MODELS)
            self.meter_model_combo.currentTextChanged.connect(self._meter_model_changed)
            self.meter_resource_combo = QtWidgets.QComboBox()
            self.meter_resource_combo.setEditable(True)
            self.nplc_spin = self._spin(0.001, 100.0, 3, 1.0)
            self.meter_refresh_button = QtWidgets.QPushButton("Refresh")
            self.meter_connect_button = QtWidgets.QPushButton("Connect")
            self.meter_disconnect_button = QtWidgets.QPushButton("Disconnect")
            self.meter_status = QtWidgets.QLabel("-")
            self.meter_status.setWordWrap(True)
            self.meter_refresh_button.clicked.connect(self.refresh_resources)
            self.meter_connect_button.clicked.connect(self.connect_meter)
            self.meter_disconnect_button.clicked.connect(self.disconnect_meter)

            self.mode_combo = QtWidgets.QComboBox()
            for mode, label in MODE_LABELS.items():
                self.mode_combo.addItem(label, mode)
            self.mode_combo.currentIndexChanged.connect(self._mode_changed)
            self.sweep_combo = QtWidgets.QComboBox()
            self.sweep_combo.addItems(["linear", "round_trip", "zero_centered", "custom_list"])
            self.start_spin = self._spin(-1e12, 1e12, 6, 0.0)
            self.end_spin = self._spin(-1e12, 1e12, 6, 0.0)
            self.step_spin = self._spin(-1e12, 1e12, 6, 1.0)
            self.start_unit_combo = QtWidgets.QComboBox()
            self.end_unit_combo = QtWidgets.QComboBox()
            self.step_unit_combo = QtWidgets.QComboBox()
            self.wait_spin = self._spin(0.0, 3600.0, 3, 0.2)
            self.average_count_spin = QtWidgets.QSpinBox()
            self.average_count_spin.setRange(1, 1000)
            self.average_count_spin.setValue(1)
            self.average_count_spin.setButtonSymbols(QtWidgets.QAbstractSpinBox.NoButtons)
            self.check_button = QtWidgets.QPushButton("Check")
            self.start_button = QtWidgets.QPushButton("Start")
            self.stop_button = QtWidgets.QPushButton("Stop")
            self.output_off_button = QtWidgets.QPushButton("Output Off")
            self.stop_button.setEnabled(False)
            self.check_button.clicked.connect(self.check_plan)
            self.start_button.clicked.connect(self.start_measurement)
            self.stop_button.clicked.connect(self.stop_measurement)
            self.output_off_button.clicked.connect(self.output_off)
            self.status_label = QtWidgets.QLabel("idle")
            self.status_label.setWordWrap(True)
            self.step_label = QtWidgets.QLabel("-")
            self.resistance_label = QtWidgets.QLabel("-")
            self.conductance_label = QtWidgets.QLabel("-")

            self.output_dir_edit = QtWidgets.QLineEdit("results")
            self.output_browse_button = QtWidgets.QPushButton("Browse")
            self.output_name_edit = QtWidgets.QLineEdit("iv_run")
            self.auto_suffix_check = QtWidgets.QCheckBox("Auto suffix")
            self.auto_suffix_check.setChecked(True)
            output_side_width = 104
            self.output_browse_button.setFixedWidth(output_side_width)
            self.auto_suffix_check.setFixedWidth(output_side_width)
            self.output_browse_button.clicked.connect(self.browse_output_dir)

            self.plot = pg.PlotWidget()
            self.plot.setBackground("#050505")
            self.plot.setMinimumHeight(320)
            self.plot.showGrid(x=True, y=True, alpha=0.25)
            self.plot_curves = {
                "forward": self.plot.plot(
                    pen=pg.mkPen("#5aa9ff", width=2),
                    symbol="o",
                    symbolSize=5,
                    symbolBrush="#5aa9ff",
                    symbolPen="#5aa9ff",
                ),
                "backward": self.plot.plot(
                    pen=pg.mkPen("#ff9f43", width=2),
                    symbol="o",
                    symbolSize=5,
                    symbolBrush="#ff9f43",
                    symbolPen="#ff9f43",
                ),
                "custom": self.plot.plot(
                    pen=pg.mkPen("#c5a3ff", width=2),
                    symbol="o",
                    symbolSize=5,
                    symbolBrush="#c5a3ff",
                    symbolPen="#c5a3ff",
                ),
            }
            self.step_text = None
            self.resistance_text = None

            self.log = QtWidgets.QPlainTextEdit()
            self.log.setReadOnly(True)
            self.log.setMinimumHeight(216)
            self.snapshot_toggle = QtWidgets.QToolButton()
            self.snapshot_toggle.setText("Field / Value")
            self.snapshot_toggle.setCheckable(True)
            self.snapshot_toggle.setChecked(True)
            self.snapshot_toggle.setToolButtonStyle(QtCore.Qt.ToolButtonTextBesideIcon)
            self.snapshot_toggle.clicked.connect(self._toggle_snapshot)
            self.snapshot_table = QtWidgets.QTableWidget(0, 2)
            self.snapshot_table.setHorizontalHeaderLabels(["Field", "Value"])
            self.snapshot_table.verticalHeader().setVisible(False)
            self.snapshot_table.horizontalHeader().setStretchLastSection(True)
            self.snapshot_table.setEditTriggers(QtWidgets.QAbstractItemView.NoEditTriggers)
            self.snapshot_table.setSelectionMode(QtWidgets.QAbstractItemView.NoSelection)

            self.left_panel_toggle = self._side_panel_toggle("<", self._toggle_left_panel)
            self.right_panel_toggle = self._side_panel_toggle(">", self._toggle_right_panel)

        def _side_panel_toggle(self, text: str, slot) -> QtWidgets.QToolButton:
            button = QtWidgets.QToolButton()
            button.setText(text)
            button.setCheckable(True)
            button.setToolButtonStyle(QtCore.Qt.ToolButtonTextOnly)
            button.setFixedWidth(24)
            button.setSizePolicy(QtWidgets.QSizePolicy.Fixed, QtWidgets.QSizePolicy.Expanding)
            button.clicked.connect(slot)
            return button

        def _apply_dark_theme(self) -> None:
            self.setStyleSheet(
                """
                QWidget {
                    background: #050505;
                    color: #e8e8e8;
                    font-size: 9pt;
                }
                QGroupBox {
                    border: 1px solid #333;
                    border-radius: 4px;
                    margin-top: 14px;
                    padding: 8px;
                    background: #0b0b0b;
                }
                QGroupBox::title {
                    subcontrol-origin: margin;
                    left: 8px;
                    padding: 0 4px;
                    color: #f0f0f0;
                }
                QLineEdit, QComboBox, QDoubleSpinBox, QSpinBox, QPlainTextEdit, QTableWidget {
                    background: #111;
                    border: 1px solid #3a3a3a;
                    border-radius: 3px;
                    color: #f2f2f2;
                    selection-background-color: #255d8f;
                }
                QPushButton, QToolButton {
                    background: #1b1b1b;
                    border: 1px solid #4a4a4a;
                    border-radius: 3px;
                    color: #f0f0f0;
                    padding: 4px 8px;
                }
                QPushButton:hover, QToolButton:hover {
                    background: #2a2a2a;
                }
                QPushButton:disabled {
                    color: #777;
                    background: #121212;
                }
                QHeaderView::section {
                    background: #151515;
                    color: #e8e8e8;
                    border: 1px solid #333;
                }
                QScrollArea {
                    border: none;
                }
                """
            )

        def _build_layout(self) -> None:
            central = QtWidgets.QWidget()
            self.setCentralWidget(central)
            root = QtWidgets.QHBoxLayout(central)
            root.setContentsMargins(8, 8, 8, 8)
            root.setSpacing(8)

            left_widget = QtWidgets.QWidget()
            left = QtWidgets.QVBoxLayout(left_widget)
            left.setSpacing(8)
            left.addWidget(self._config_group())
            left.addWidget(self._source_group())
            left.addWidget(self._meter_group())
            left.addStretch(1)
            self.left_content = QtWidgets.QScrollArea()
            self.left_content.setWidgetResizable(True)
            self.left_content.setHorizontalScrollBarPolicy(QtCore.Qt.ScrollBarAlwaysOff)
            self.left_content.setWidget(left_widget)
            self.left_content.setMinimumWidth(260)
            self.left_content.setMaximumWidth(290)

            self.left_panel = QtWidgets.QWidget()
            left_shell = QtWidgets.QHBoxLayout(self.left_panel)
            left_shell.setContentsMargins(0, 0, 0, 0)
            left_shell.setSpacing(4)
            left_shell.addWidget(self.left_content, 1)
            left_shell.addWidget(self.left_panel_toggle)

            center_widget = QtWidgets.QWidget()
            center = QtWidgets.QVBoxLayout(center_widget)
            center.setSpacing(8)
            center.addWidget(self._measurement_group(), 0)
            center.addWidget(self._plot_status_bar(), 0)
            center.addWidget(self.plot, 1)

            self.right_panel = QtWidgets.QWidget()
            right_shell = QtWidgets.QHBoxLayout(self.right_panel)
            right_shell.setContentsMargins(0, 0, 0, 0)
            right_shell.setSpacing(4)
            right_shell.addWidget(self.right_panel_toggle)
            self.right_content = QtWidgets.QWidget()
            right = QtWidgets.QVBoxLayout(self.right_content)
            right.setSpacing(8)
            right.addWidget(self.log, 0)
            right.addWidget(self.snapshot_toggle, 0)
            right.addWidget(self.snapshot_table, 1)
            right_shell.addWidget(self.right_content, 1)

            root.addWidget(self.left_panel, 0)
            root.addWidget(center_widget, 1)
            root.addWidget(self.right_panel, 0)
            self.resize(1240, 760)

        def _config_group(self):
            group = QtWidgets.QGroupBox("Config")
            layout = QtWidgets.QVBoxLayout(group)
            row1 = QtWidgets.QHBoxLayout()
            row1.addWidget(self.config_path_edit, 1)
            row1.addWidget(self.browse_button)
            row2 = QtWidgets.QHBoxLayout()
            for button in (self.load_button, self.save_button):
                row2.addWidget(button)
            row3 = QtWidgets.QHBoxLayout()
            for button in (self.connect_all_button, self.disconnect_all_button):
                row3.addWidget(button)
            layout.addLayout(row1)
            layout.addLayout(row2)
            layout.addLayout(row3)
            return group

        def _source_group(self):
            group = QtWidgets.QGroupBox("Source")
            form = QtWidgets.QFormLayout(group)
            form.addRow("Device", self.source_model_combo)
            form.addRow("Resource", self._resource_row(self.source_resource_combo, self.source_refresh_button))
            buttons = QtWidgets.QHBoxLayout()
            for button in (self.source_connect_button, self.source_disconnect_button):
                buttons.addWidget(button)
            form.addRow("", buttons)
            return group

        def _meter_group(self):
            group = QtWidgets.QGroupBox("Meter")
            form = QtWidgets.QFormLayout(group)
            form.addRow("Device", self.meter_model_combo)
            form.addRow("Resource", self._resource_row(self.meter_resource_combo, self.meter_refresh_button))
            form.addRow("NPLC", self.nplc_spin)
            buttons = QtWidgets.QHBoxLayout()
            for button in (self.meter_connect_button, self.meter_disconnect_button):
                buttons.addWidget(button)
            form.addRow("", buttons)
            return group

        def _measurement_group(self):
            group = QtWidgets.QGroupBox("Measurement")
            layout = QtWidgets.QHBoxLayout(group)
            left = QtWidgets.QFormLayout()
            left.addRow("Mode", self.mode_combo)
            left.addRow("Sweep", self.sweep_combo)
            left.addRow("Start", self._quantity_row(self.start_spin, self.start_unit_combo))
            left.addRow("End", self._quantity_row(self.end_spin, self.end_unit_combo))
            left.addRow("Step", self._quantity_row(self.step_spin, self.step_unit_combo))
            left.addRow("Wait time (s)", self.wait_spin)
            left.addRow("Average count", self.average_count_spin)

            right_widget = QtWidgets.QWidget()
            right = QtWidgets.QVBoxLayout(right_widget)
            right.setContentsMargins(0, 0, 0, 0)
            right.addWidget(self._output_group())
            right.addWidget(self._run_group())
            right.addStretch(1)

            layout.addLayout(left, 1)
            layout.addWidget(right_widget, 1)
            return group

        def _output_group(self):
            group = QtWidgets.QGroupBox("Output")
            form = QtWidgets.QFormLayout(group)
            form.addRow("Directory", self._resource_row(self.output_dir_edit, self.output_browse_button))
            form.addRow("File", self._resource_row(self.output_name_edit, self.auto_suffix_check))
            return group

        def _run_group(self):
            group = QtWidgets.QGroupBox("Run")
            layout = QtWidgets.QVBoxLayout(group)
            run_row = QtWidgets.QHBoxLayout()
            for button in (self.start_button, self.stop_button, self.output_off_button):
                run_row.addWidget(button)
            layout.addLayout(run_row)
            return group

        def _plot_status_bar(self):
            widget = QtWidgets.QWidget()
            layout = QtWidgets.QHBoxLayout(widget)
            layout.setContentsMargins(0, 0, 0, 0)
            layout.addWidget(QtWidgets.QLabel("Status"))
            layout.addWidget(self.status_label, 2)
            layout.addWidget(QtWidgets.QLabel("Point"))
            layout.addWidget(self.step_label, 1)
            layout.addWidget(QtWidgets.QLabel("Resistance"))
            layout.addWidget(self.resistance_label, 1)
            layout.addWidget(QtWidgets.QLabel("Conductance"))
            layout.addWidget(self.conductance_label, 1)
            return widget

        def _quantity_row(self, spin, unit):
            widget = QtWidgets.QWidget()
            layout = QtWidgets.QHBoxLayout(widget)
            layout.setContentsMargins(0, 0, 0, 0)
            layout.addWidget(spin, 1)
            layout.addWidget(unit)
            return widget

        def _resource_row(self, primary, button):
            widget = QtWidgets.QWidget()
            layout = QtWidgets.QHBoxLayout(widget)
            layout.setContentsMargins(0, 0, 0, 0)
            layout.addWidget(primary, 1)
            layout.addWidget(button)
            return widget

        def _spin(self, minimum: float, maximum: float, decimals: int, value: float):
            spin = QtWidgets.QDoubleSpinBox()
            spin.setRange(minimum, maximum)
            spin.setDecimals(3)
            spin.setValue(value)
            spin.setKeyboardTracking(False)
            spin.setButtonSymbols(QtWidgets.QAbstractSpinBox.NoButtons)
            return spin

        def _source_key(self) -> str:
            source_ref, _ = self._active_refs(self.config)
            return source_ref.split(".", 1)[1]

        def _meter_key(self) -> str:
            _, meter_ref = self._active_refs(self.config)
            return meter_ref.split(".", 1)[1]

        def _active_refs(self, config: dict[str, Any]) -> tuple[str, str]:
            mode = str(config["measurements"]["iv"].get("mode", "dc_iv"))
            role_key = "vi" if mode == "dc_vi" else "iv"
            role = config.get("roles", {}).get(role_key) or config["roles"]["iv"]
            return str(role["source"]), str(role["measure"])

        def _mode(self) -> str:
            return str(self.mode_combo.currentData() or "dc_vi")

        def _set_mode(self, mode: str) -> None:
            for index in range(self.mode_combo.count()):
                if self.mode_combo.itemData(index) == mode:
                    self.mode_combo.setCurrentIndex(index)
                    return

        def _mode_units(self, mode: str) -> list[str]:
            return CURRENT_UNITS if mode == "dc_vi" else VOLTAGE_UNITS

        def _default_unit(self, mode: str) -> str:
            return "mA" if mode == "dc_vi" else "mV"

        def _replace_units(self, combo, units: list[str], preferred: str | None = None) -> None:
            current = preferred or combo.currentText()
            combo.blockSignals(True)
            combo.clear()
            combo.addItems(units)
            index = combo.findText(current)
            combo.setCurrentIndex(index if index >= 0 else 0)
            combo.blockSignals(False)

        def _mode_changed(self) -> None:
            mode = self._mode()
            units = self._mode_units(mode)
            default_unit = self._default_unit(mode)
            for combo in (self.start_unit_combo, self.end_unit_combo, self.step_unit_combo):
                preferred = combo.currentText() if combo.currentText() in units else default_unit
                self._replace_units(combo, units, preferred)
            self._refresh_plot_labels()

        def _meter_model_changed(self, model: str) -> None:
            for key, cfg in self.config.get("instruments", {}).get("meter", {}).items():
                if str(cfg.get("model", "")).upper() == model:
                    self.meter_resource_combo.setCurrentText(str(cfg.get("resource", "")))
                    return

        def _load_fields(self) -> None:
            settings = self.config["measurements"]["iv"]
            mode = str(settings.get("mode", "dc_vi"))
            self._set_mode(mode)
            self._mode_changed()

            source_key = self._source_key()
            meter_key = self._meter_key()
            source = self.config["instruments"]["source"][source_key]
            meter = self.config["instruments"]["meter"][meter_key]
            self.source_model_combo.setCurrentText(str(source.get("model", SOURCE_MODELS[0])).upper())
            self.source_resource_combo.setCurrentText(str(source.get("resource", "")))
            self.meter_model_combo.setCurrentText(str(meter.get("model", METER_MODELS[0])).upper())
            self.meter_resource_combo.setCurrentText(str(meter.get("resource", "")))

            scan = settings["scan"]
            self.sweep_combo.setCurrentText(str(scan.get("pattern", "linear")))
            default_unit = self._default_unit(mode)
            for key, spin, unit in (
                ("start", self.start_spin, self.start_unit_combo),
                ("stop", self.end_spin, self.end_unit_combo),
                ("step", self.step_spin, self.step_unit_combo),
            ):
                value, unit_text = _quantity_value(scan, key, 0.0, default_unit)
                spin.setValue(value)
                self._replace_units(unit, self._mode_units(mode), unit_text)

            timing = settings.get("timing", {})
            self.wait_spin.setValue(float(timing.get("settle_s", 0.2)))
            self.average_count_spin.setValue(int(timing.get("average_count", 1)))
            self.nplc_spin.setValue(float(timing.get("nplc", 1.0)))
            output = settings.get("output", {})
            self.output_dir_edit.setText(str(output.get("dir", "results")))
            self.output_name_edit.setText(str(output.get("filename", "iv_run")))
            self.auto_suffix_check.setChecked(bool(output.get("auto_timestamp_suffix", True)))
            self._refresh_plot_labels()

        def _config_from_fields(self) -> dict[str, Any]:
            config = copy.deepcopy(self.config)
            source_key = self._source_key()
            meter_key = self._meter_key_for_selected_model(config)
            source_model = self.source_model_combo.currentText().strip().upper() or "YOKOGAWA_GS210"
            meter_model = self.meter_model_combo.currentText().strip().upper()
            config["instruments"]["source"][source_key]["model"] = source_model
            config["instruments"]["source"][source_key]["resource"] = self.source_resource_combo.currentText().strip()
            config["instruments"]["meter"][meter_key]["model"] = meter_model
            config["instruments"]["meter"][meter_key]["resource"] = self.meter_resource_combo.currentText().strip()
            if meter_model == "ADCMT_7461A":
                config["instruments"]["meter"][meter_key].setdefault("command_language", "scpi")
            for role_key in ("iv", "vi"):
                config["roles"].setdefault(role_key, {})
                config["roles"][role_key]["source"] = f"source.{source_key}"
                config["roles"][role_key]["measure"] = f"meter.{meter_key}"

            settings = config["measurements"]["iv"]
            mode = self._mode()
            settings["mode"] = mode
            scan = settings.setdefault("scan", {})
            scan["pattern"] = self.sweep_combo.currentText()
            _set_quantity(scan, "start", self.start_spin.value(), self.start_unit_combo.currentText())
            _set_quantity(scan, "stop", self.end_spin.value(), self.end_unit_combo.currentText())
            _set_quantity(scan, "step", self.step_spin.value(), self.step_unit_combo.currentText())
            scan.setdefault("repeat", 1)
            scan.setdefault("custom_points", [])

            timing = settings.setdefault("timing", {})
            timing["pre_delay_s"] = float(timing.get("pre_delay_s", 0.1))
            timing["settle_s"] = self.wait_spin.value()
            timing["post_zero_delay_s"] = float(timing.get("post_zero_delay_s", 0.1))
            timing["ramp_step_wait_s"] = float(timing.get("ramp_step_wait_s", 0.02))
            timing["nplc"] = self.nplc_spin.value()
            timing["average_count"] = self.average_count_spin.value()
            timing["measure_timeout_s"] = float(timing.get("measure_timeout_s", 10.0))
            timing["timing_mode"] = "software"
            settings.setdefault("measure", {})["auto_range"] = True
            settings["output"] = {
                "dir": self.output_dir_edit.text().strip() or "results",
                "filename": self.output_name_edit.text().strip() or "iv_run",
                "auto_timestamp_suffix": self.auto_suffix_check.isChecked(),
            }

            safety = settings.setdefault("safety", {})
            source_unit = self.start_unit_combo.currentText()
            source_limit_value = max(abs(self.start_spin.value()), abs(self.end_spin.value()))
            _set_quantity(safety, "max_abs_source", source_limit_value, source_unit)
            _set_quantity(safety, "ramp_step", abs(self.step_spin.value()), self.step_unit_combo.currentText())
            self._ensure_compliance_quantity(safety, mode)
            safety["stop_on_compliance"] = True
            safety["on_finish"] = "ramp_to_zero_then_off"
            safety["on_stop"] = "ramp_to_zero_then_off"
            safety["on_error"] = "output_off"
            safety["output_off_on_finish"] = True
            return config

        def _meter_key_for_selected_model(self, config: dict[str, Any]) -> str:
            model = self.meter_model_combo.currentText().strip().upper()
            for key, cfg in config.get("instruments", {}).get("meter", {}).items():
                if str(cfg.get("model", "")).upper() == model:
                    return key
            current = self._meter_key()
            config["instruments"].setdefault("meter", {}).setdefault(current, {})
            return current

        def _ensure_compliance_quantity(self, safety: dict[str, Any], mode: str) -> None:
            compliance = safety.get("compliance", {})
            unit = compliance.get("unit") if isinstance(compliance, dict) else None
            family = _unit_family(str(unit or ""))
            if mode == "dc_vi":
                if family != "voltage":
                    _set_quantity(safety, "compliance", 1.0, "V")
            else:
                if family != "current":
                    _set_quantity(safety, "compliance", 10.0, "uA")

        def browse_config(self) -> None:
            path, _ = QtWidgets.QFileDialog.getOpenFileName(self, "Load Config", str(Path.cwd()), "JSON Files (*.json)")
            if path:
                self.config_path_edit.setText(path)

        def browse_output_dir(self) -> None:
            path = QtWidgets.QFileDialog.getExistingDirectory(self, "Output Directory", self.output_dir_edit.text() or str(Path.cwd()))
            if path:
                self.output_dir_edit.setText(path)

        def load_preset(self, path: str) -> None:
            self.config_path_edit.setText(str(Path(path)))
            self.load_config()

        def connect_all(self) -> None:
            self.connect_source()
            self.connect_meter()

        def disconnect_all(self) -> None:
            self._safe_zero_output_off(raise_errors=False)
            self.experiment.disconnect_all()
            self.source_status.setText("local")
            self.meter_status.setText("local")
            self.append_log("Disconnected all.")

        def load_config(self) -> None:
            try:
                self.config_path = Path(self.config_path_edit.text())
                self.config = load_config(self.config_path)
                self.experiment.config = self.config
                self._load_fields()
                write_last_config_path(self.config_path)
                self.append_log(f"Loaded config: {self.config_path}")
            except Exception as e:
                QtWidgets.QMessageBox.warning(self, "Config Error", str(e))
                self.append_log(f"Config error: {e}")

        def save_config(self) -> None:
            try:
                self.config = self._config_from_fields()
                save_config(self.config, self.config_path_edit.text())
                self.experiment.config = self.config
                write_last_config_path(self.config_path_edit.text())
                self.append_log(f"Saved config: {self.config_path_edit.text()}")
            except Exception as e:
                QtWidgets.QMessageBox.warning(self, "Config Error", str(e))
                self.append_log(f"Config error: {e}")

        def refresh_resources(self) -> None:
            try:
                resources = list(list_visa_resources())
                for combo in (self.source_resource_combo, self.meter_resource_combo):
                    current = combo.currentText()
                    combo.clear()
                    combo.addItems(resources)
                    combo.setCurrentText(current)
                self.append_log("Resources refreshed.")
            except Exception as e:
                QtWidgets.QMessageBox.warning(self, "Resource Error", str(e))
                self.append_log(f"Resource error: {e}")

        def _apply_config(self) -> None:
            self.config = self._config_from_fields()
            self.experiment.config = self.config

        def connect_source(self) -> None:
            try:
                self._apply_config()
                source_ref, _ = self._active_refs(self.config)
                source = self.experiment.connect_device(source_ref)
                idn = source.identify()
                source.output_off()
                source.set_level(0.0)
                source.output_off()
                self.source_status.setText(f"{idn}\noutput={source.output_state()} level={_short(source.read_level())}")
                self.append_log(f"Connected {source_ref}: {idn}")
            except Exception as e:
                QtWidgets.QMessageBox.critical(self, "Connect Error", str(e))
                self.append_log(f"Source connect error: {e}")

        def disconnect_source(self) -> None:
            source_ref, _ = self._active_refs(self.config)
            self._safe_zero_output_off(raise_errors=False)
            affected = self.experiment.disconnect_device(source_ref)
            self._mark_disconnected(affected or [source_ref])
            self.append_log(f"Disconnected {', '.join(affected or [source_ref])}.")

        def connect_meter(self) -> None:
            try:
                self._apply_config()
                _, meter_ref = self._active_refs(self.config)
                meter = self.experiment.connect_device(meter_ref)
                idn = meter.identify()
                status = self._connect_status(meter)
                self.meter_status.setText(f"{idn}\n{status}")
                self.append_log(f"Connected {meter_ref}: {idn}")
            except Exception as e:
                QtWidgets.QMessageBox.critical(self, "Connect Error", str(e))
                self.append_log(f"Meter connect error: {e}")

        def _connect_status(self, device) -> str:
            method = getattr(device, "connect_status", None)
            if method is not None:
                return str(method())
            try:
                device.write("*CLS")
                return str(device.query("SYST:ERR?"))
            except Exception as e:
                return f"status unavailable: {e}"

        def disconnect_meter(self) -> None:
            _, meter_ref = self._active_refs(self.config)
            affected = self.experiment.disconnect_device(meter_ref)
            self._mark_disconnected(affected or [meter_ref])
            self.append_log(f"Disconnected {', '.join(affected or [meter_ref])}.")

        def _mark_disconnected(self, refs: list[str]) -> None:
            if any(ref.startswith("source.") for ref in refs):
                self.source_status.setText("local")
            if any(ref.startswith("meter.") for ref in refs):
                self.meter_status.setText("local")

        def check_plan(self) -> None:
            try:
                config = self._config_from_fields()
                plan = iv_plan_from_config(config)
                path = output_path(config, "iv")
                self.status_label.setText(plan.summary)
                self.append_log(f"Check OK: {plan.summary} -> {path}")
            except Exception as e:
                QtWidgets.QMessageBox.warning(self, "Plan Error", str(e))
                self.append_log(f"Plan error: {e}")

        def start_measurement(self) -> None:
            try:
                self._apply_config()
                plan = iv_plan_from_config(self.config)
                source_ref, meter_ref = self._active_refs(self.config)
                connected = self.experiment.connected_devices()
                if not connected.get(source_ref) or not connected.get(meter_ref):
                    raise RuntimeError("Connect Source and Meter before starting.")
                out = output_path(self.config, "iv")
                self.rows.clear()
                self._update_plot()
                self.thread = QtCore.QThread()
                self.worker = MeasurementWorker(experiment=self.experiment, config=self.config, output=str(out))
                self.worker.moveToThread(self.thread)
                self.thread.started.connect(self.worker.run)
                self.worker.status_changed.connect(self.handle_status)
                self.worker.point_ready.connect(self.handle_point)
                self.worker.error_occurred.connect(self.handle_error)
                self.worker.finished.connect(self.handle_finished)
                self.worker.finished.connect(self.thread.quit)
                self.worker.finished.connect(self.worker.deleteLater)
                self.thread.finished.connect(self._thread_finished)
                self.thread.finished.connect(self.thread.deleteLater)
                self.thread.start()
                self.start_button.setEnabled(False)
                self.stop_button.setEnabled(True)
                self.append_log(f"Started {plan.summary} -> {out}")
            except Exception as e:
                QtWidgets.QMessageBox.critical(self, "Start Error", str(e))
                self.append_log(f"Start error: {e}")

        def stop_measurement(self) -> None:
            if self.worker is not None:
                self.worker.stop()
                self.append_log("Stop requested.")

        def output_off(self) -> None:
            try:
                self._apply_config()
                source = self._safe_zero_output_off(connect_if_missing=True)
                if source is None:
                    raise RuntimeError("Source is not available.")
                self.source_status.setText(f"output=False level={_short(source.read_level())}")
                self.append_log("Source output off and level set to zero.")
            except Exception as e:
                QtWidgets.QMessageBox.critical(self, "Output Off Error", str(e))
                self.append_log(f"Output off error: {e}")

        def _safe_zero_output_off(self, *, connect_if_missing: bool = False, raise_errors: bool = True):
            try:
                source_ref, _ = self._active_refs(self.config)
                try:
                    source = self.experiment.session.require(source_ref)
                except RuntimeError:
                    if not connect_if_missing:
                        return None
                    source = self.experiment.connect_device(source_ref)
                source.output_off()
                source.set_level(0.0)
                source.output_off()
                return source
            except Exception as e:
                if raise_errors:
                    raise
                self.append_log(f"Output off/zero skipped: {e}")
                return None

        def handle_status(self, status: str) -> None:
            self.status_label.setText(status)
            self.append_log(status)

        def handle_error(self, message: str) -> None:
            self.append_log(f"Error: {message}")
            QtWidgets.QMessageBox.critical(self, "Measurement Error", message)

        def handle_point(self, point) -> None:
            row = point.row
            self.rows.append(row)
            self._update_plot()
            self._update_snapshot(row)

        def handle_finished(self, rows) -> None:
            self.append_log(f"Finished. {len(rows)} points collected.")
            self.status_label.setText(f"finished {len(rows)} points")
            self.start_button.setEnabled(True)
            self.stop_button.setEnabled(False)
            self.append_log("Source returned to zero/output off. Devices remain remote.")

        def _thread_finished(self) -> None:
            self.thread = None
            self.worker = None

        def _refresh_plot_labels(self) -> None:
            mode = self._mode()
            if mode == "dc_vi":
                self.plot.setLabel("bottom", "Current", units="A")
                self.plot.setLabel("left", "Voltage", units="V")
            else:
                self.plot.setLabel("bottom", "Voltage", units="V")
                self.plot.setLabel("left", "Current", units="A")

        def _plot_points(self) -> list[tuple[float, float, str]]:
            mode = self._mode()
            points: list[tuple[float, float, str]] = []
            for row in self.rows:
                measured_v = row.get("measured_V")
                measured_a = row.get("measured_A")
                if measured_v is None or measured_a is None:
                    continue
                if mode == "dc_vi":
                    x, y = float(measured_a), float(measured_v)
                else:
                    x, y = float(measured_v), float(measured_a)
                points.append((x, y, str(row.get("direction") or "forward")))
            return points

        def _plot_groups(self) -> dict[str, list[tuple[float, float]]]:
            groups = {key: [] for key in self.plot_curves}
            last_point: tuple[float, float] | None = None
            last_direction: str | None = None
            for x, y, direction in self._plot_points():
                key = direction if direction in groups else "custom"
                if last_point is not None and key != last_direction:
                    if groups[key]:
                        groups[key].append((float("nan"), float("nan")))
                    groups[key].append(last_point)
                groups[key].append((x, y))
                last_point = (x, y)
                last_direction = key
            return groups

        def _update_plot(self) -> None:
            self._refresh_plot_labels()
            groups = self._plot_groups()
            count = len(self._plot_points())
            has_points = False
            for key, curve in self.plot_curves.items():
                points = groups.get(key, [])
                xs = [x for x, _ in points]
                ys = [y for _, y in points]
                curve.setData(xs, ys)
                has_points = has_points or bool(points)
            if has_points:
                self.plot.enableAutoRange(axis="xy", enable=True)
                self.plot.autoRange()
            resistance = _resistance_from_rows(self.rows)
            conductance = None if resistance in (None, 0) else 1.0 / resistance
            point_text = f"{count}/{self.rows[-1].get('total_points', '-')}" if self.rows else "-"
            resistance_text = format_resistance(resistance)
            conductance_text = format_conductance(conductance)
            self.step_label.setText(point_text)
            self.resistance_label.setText(resistance_text)
            self.conductance_label.setText(conductance_text)

        def _position_plot_text(self) -> None:
            if self.step_text is None or self.resistance_text is None:
                return
            view = self.plot.getViewBox().viewRange()
            x_min, x_max = view[0]
            y_min, y_max = view[1]
            self.step_text.setPos(x_min, y_max)
            self.resistance_text.setPos(x_max, y_max)

        def _update_snapshot(self, row: dict[str, Any]) -> None:
            keys = [key for key in SNAPSHOT_FIELDS if key in row]
            self.snapshot_table.setRowCount(len(keys))
            for index, key in enumerate(keys):
                self.snapshot_table.setItem(index, 0, QtWidgets.QTableWidgetItem(str(key)))
                value = row.get(key)
                text = "" if value is None else _short(value)
                self.snapshot_table.setItem(index, 1, QtWidgets.QTableWidgetItem(text))
            self.snapshot_table.resizeColumnsToContents()

        def _toggle_snapshot(self) -> None:
            visible = self.snapshot_toggle.isChecked()
            self.snapshot_table.setVisible(visible)

        def _toggle_left_panel(self) -> None:
            visible = not self.left_panel_toggle.isChecked()
            self.left_content.setVisible(visible)
            self.left_panel_toggle.setText("<" if visible else ">")

        def _toggle_right_panel(self) -> None:
            visible = not self.right_panel_toggle.isChecked()
            self.right_content.setVisible(visible)
            self.right_panel_toggle.setText(">" if visible else "<")

        def append_log(self, text: str) -> None:
            self.log.appendPlainText(str(text))
            self.log.verticalScrollBar().setValue(self.log.verticalScrollBar().maximum())

        def closeEvent(self, event) -> None:
            if self.worker is not None:
                self.worker.stop()
            if self.thread is not None and self.thread.isRunning():
                self.thread.quit()
                if not self.thread.wait(15000):
                    self.append_log("Close delayed: measurement thread is still stopping.")
                    event.ignore()
                    return
            try:
                self._safe_zero_output_off(raise_errors=False)
                self.experiment.disconnect_all()
            finally:
                super().closeEvent(event)

    app = QtWidgets.QApplication(sys.argv)
    window = IVGui()
    window.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
