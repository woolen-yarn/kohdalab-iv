# KohdaLab IV Measurement Specification / 測定仕様

Version: 1.0

## 日本語

この文書は KohdaLab IV の現行仕様をまとめたものです。KohdaLab IV は、
実機を使った DC IV/VI sweep を GUI、CLI、Jupyter Notebook から共通の測定
コアで扱うためのプログラムです。

### 現在の対象装置

標準構成:

- Source: Yokogawa GS210, `YOKOGAWA_GS210`
- Source VISA resource: `GPIB0::2::INSTR`
- Meter: Agilent/HP 34401A, `AGILENT_34401A`
- Meter VISA resource: `GPIB0::26::INSTR`
- Optional meter model: `KEYSIGHT_34411A`

装置の capability と hard limit は TOML に置きます。

```text
src/kohdalab_iv/instruments/sources/specs.toml
src/kohdalab_iv/instruments/meters/specs.toml
```

測定ごとの設定は JSON です。

```text
config/default.json
```

標準 config は `config/default.json` に一本化します。一時的な smoke config や
example config は、標準 workflow として採用する場合以外は追加しません。

### 測定 mode

対応する DC sweep mode は 2 つです。

`dc_vi`

- GS210 で電流を source します。
- DMM で電圧を measure します。
- live plot の x 軸は current です。
- live plot の y 軸は voltage です。
- 抵抗は `voltage_V / current_A` で計算します。

`dc_iv`

- GS210 で電圧を source します。
- DMM で電流を measure します。
- live plot の x 軸は voltage です。
- live plot の y 軸は current です。
- 抵抗は `voltage_V / current_A` で計算します。

config には将来用に AC field を残せますが、現時点の対応 workflow は
software timing の DC sweep です。

### Sweep pattern

GUI では次の pattern を選べます。

- `linear`: start から stop まで一方向に sweep
- `round_trip`: start から stop まで行き、その後戻る
- `zero_centered`: `0 -> +max -> -max -> 0` の点列
- `custom_list`: 明示的な点列用

現在の default は `dc_vi` の linear current sweep です。

```json
"start": {"value": -100.0, "unit": "mA"},
"stop": {"value": 100.0, "unit": "mA"},
"step": {"value": 10.0, "unit": "mA"}
```

### Unit

GUI と JSON config は engineering unit を受け付けます。

Voltage:

- `nV`
- `uV`
- `mV`
- `V`

Current:

- `pA`
- `nA`
- `uA`
- `mA`
- `A`

scan plan 生成時に SI 値へ変換します。CSV は target、読み取った
`voltage_V/current_A`、および `resistance_Ohm/conductance_S` に絞ります。GUI の抵抗とコンダクタンス表示は
読みやすさのために自動で scale します。

- `120 mOhm`
- `1.25 Ohm`
- `4.7 kOhm`
- `2.1 MOhm`
- `1.2 mS`
- `2.5 uS`

### GUI layout

GUI は TRKR 風の 3 パネル構成です。

Left panel:

- Config
- Source
- Meter

Center panel:

- Measurement
- Output
- Run
- Plot

source target が増える点を `forward`、減る点を `backward` とし、plot では
別色で表示します。

Right panel:

- Log
- 折りたたみ可能な最新行の Field/Value table

#### Config panel

Controls:

- Browse
- Load
- Save
- All Connect
- All Disconnect

GUI は最後に開いた config path を記憶します。記憶した path が存在しない
場合は `config/default.json` に fallback します。

#### Source panel

Controls:

- Device: `YOKOGAWA_GS210`
- Resource
- Refresh
- Connect
- Disconnect

Source connect 時には、GS210 を安全側に初期化します。具体的には output off、
level zero、output off を実行します。

#### Meter panel

Controls:

- Device: `AGILENT_34401A` or `KEYSIGHT_34411A`
- Resource
- Refresh
- NPLC
- Connect
- Disconnect

`NPLC` は `timing.nplc` に対応し、DMM の測定設定として送られます。

#### Measurement panel

Controls:

- Mode
- Sweep
- Start
- End
- Step
- Wait time
- Average count

Start、End、Step は必ず unit 付きで扱います。選択できる unit は mode に
応じて変わります。

`Wait time` は `timing.settle_s` に対応します。source target を設定した後、
DMM を読む前に待つ時間です。

`Average count` は `timing.average_count` に対応します。DMM を複数回読み、
その算術平均を 1 点の値として使います。

#### Output panel

Controls:

- Directory
- Browse
- File
- Auto suffix

Auto suffix が有効な場合、出力ファイル名に timestamp を付けます。CSV は
1 点ごとに flush するので、Stop した場合も測定済みデータが残ります。

#### Run panel

Controls:

- Start
- Stop
- Output Off

Start には Source と Meter が接続済みである必要があります。Stop は安全な
停止を要求し、測定コア側で 0 への ramp と output off を行います。

Output Off は source output off、level zero、source output off をすぐに
実行します。

### 安全動作

通常終了:

1. 設定された ramp step で source を 0 に戻します。
2. `post_zero_delay_s` だけ待ちます。
3. output off します。
4. 装置は connected/remote のままにします。

Stop:

1. 次の interrupt 可能な点で停止します。
2. source を 0 に戻します。
3. output off します。
4. 装置は connected/remote のままにします。

Error:

1. output off を優先します。
2. error を caller または GUI に通知します。

GUI close、Source Disconnect、All Disconnect:

1. output off と zero を試します。
2. 接続中の装置を local に戻します。
3. VISA resource を close します。

34401A には `SYST:LOC` を送りません。34401A の RS-232 専用 command error を
避けるため、GPIB GTL で local に戻します。

### CSV output

CSV column は `src/kohdalab_iv/api/measurement_rows.py` で定義します。

CSV は意図的に compact にしています。内部 row には追加 metadata を残しますが、
保存ファイルは plot と解析に必要な値に絞ります。

保存する field:

- `timestamp`
- `elapsed_s`
- `point_index`
- `direction`
- `target_value`
- `target_unit`
- `voltage_V`
- `current_A`
- `resistance_Ohm`
- `conductance_S`

resistance と conductance は CSV に SI 値として保存し、GUI の plot 上部では
読みやすい engineering unit で表示します。

`direction` は sweep pattern の往路/復路ではなく、source target の増減で決まります。
target が前の点より増えた場合は `forward`、減った場合は `backward` です。
最初の点は次の点への移動方向で決めます。

### CLI

Config check:

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run kohdalab-iv --config config/default.json check-config
```

VISA resource list:

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run kohdalab-iv list-resources
```

Measurement:

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run kohdalab-iv --config config/default.json measure iv
```

### GUI

Desktop VBS から起動:

```text
Desktop\KohdaLab_IV_GUI.vbs
```

プロジェクトディレクトリから起動:

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run --extra gui kohdalab-iv-gui
```

### Jupyter

```powershell
.\run_jupyter.ps1
```

Notebook 例:

```python
from pathlib import Path

from kohdalab_iv.api.config import load_config
from kohdalab_iv.api.experiment import Experiment
from kohdalab_iv.api.scan_plan import iv_plan_from_config
from kohdalab_iv.api.notebook import format_point, make_iv_live_update

config = load_config(Path("config/default.json"))
plan = iv_plan_from_config(config)
experiment = Experiment(config, auto_connect=False)

experiment.connect_device(plan.source_ref)
experiment.connect_device(plan.measure_ref)

live = make_iv_live_update(x_key="current_A", y_key="voltage_V")
rows = experiment.run_iv(
    plan=plan,
    on_point=lambda point: (print(format_point(point)), live(point)),
)
```

### 実装 layer

```text
apps / CLI / notebooks
  -> kohdalab_iv.api
  -> kohdalab_iv.interfaces
  -> kohdalab_iv.instruments
```

`instruments`

- 低レベルの SCPI/VISA command と model ごとの quirks

`api`

- config loading
- unit parsing
- scan-plan generation
- device session management
- measurement execution
- CSV row generation

`apps`

- GUI entry point と presentation behavior

### 確認 command

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run --group dev pytest
& "$env:USERPROFILE\.local\bin\uv.exe" run --extra gui python -c "from kohdalab_iv.apps.iv_gui import main; print('iv_gui import OK')"
& "$env:USERPROFILE\.local\bin\uv.exe" run kohdalab-iv --config config/default.json check-config
```

---

## English

This document describes the current KohdaLab IV implementation. KohdaLab IV is
designed for DC IV/VI sweeps with real instruments, using the same measurement
core from the GUI, CLI, and Jupyter notebooks.

### Current Hardware Target

Standard setup:

- Source: Yokogawa GS210, `YOKOGAWA_GS210`
- Source VISA resource: `GPIB0::2::INSTR`
- Meter: Agilent/HP 34401A, `AGILENT_34401A`
- Meter VISA resource: `GPIB0::26::INSTR`
- Optional meter model: `KEYSIGHT_34411A`

Instrument capability and hard-limit metadata live in TOML files:

```text
src/kohdalab_iv/instruments/sources/specs.toml
src/kohdalab_iv/instruments/meters/specs.toml
```

Per-run settings live in JSON:

```text
config/default.json
```

`config/default.json` is the only standard project config. Temporary smoke or
example configs should not be added unless they are intentionally promoted to
the standard workflow.

### Measurement Modes

The project supports two DC sweep modes.

`dc_vi`

- Source current with the GS210.
- Measure voltage with the DMM.
- The live plot x-axis is current.
- The live plot y-axis is voltage.
- Resistance is calculated as `voltage_V / current_A`.

`dc_iv`

- Source voltage with the GS210.
- Measure current with the DMM.
- The live plot x-axis is voltage.
- The live plot y-axis is current.
- Resistance is calculated as `voltage_V / current_A`.

AC fields may remain in config for future compatibility, but the current
supported workflow is software-timed DC sweep.

### Sweep Patterns

The GUI exposes these patterns:

- `linear`: sweep from start to stop in one direction
- `round_trip`: sweep from start to stop, then return
- `zero_centered`: use points ordered as `0 -> +max -> -max -> 0`
- `custom_list`: reserved for explicit point lists

The current default is `dc_vi` with a linear current sweep:

```json
"start": {"value": -100.0, "unit": "mA"},
"stop": {"value": 100.0, "unit": "mA"},
"step": {"value": 10.0, "unit": "mA"}
```

### Unit Handling

The GUI and JSON config accept engineering units.

Voltage:

- `nV`
- `uV`
- `mV`
- `V`

Current:

- `pA`
- `nA`
- `uA`
- `mA`
- `A`

The scan-plan builder converts all quantities to SI values before measurement.
CSV output is limited to target, read `voltage_V/current_A`, and
`resistance_Ohm/conductance_S`. GUI resistance
and conductance displays are scaled for readability:

- `120 mOhm`
- `1.25 Ohm`
- `4.7 kOhm`
- `2.1 MOhm`
- `1.2 mS`
- `2.5 uS`

### GUI Layout

The GUI follows a TRKR-like three-panel layout.

Left panel:

- Config
- Source
- Meter

Center panel:

- Measurement
- Output
- Run
- Plot

Points where the source target increases are labeled `forward`; points where it
decreases are labeled `backward`. The plot draws them in different colors.

Right panel:

- Log
- Collapsible latest-row Field/Value table

#### Config Panel

Controls:

- Browse
- Load
- Save
- All Connect
- All Disconnect

The GUI remembers the last opened config path. If the remembered path no longer
exists, it falls back to `config/default.json`.

#### Source Panel

Controls:

- Device: `YOKOGAWA_GS210`
- Resource
- Refresh
- Connect
- Disconnect

Source connect initializes the GS210 to a safe state by setting output off,
level zero, and output off again.

#### Meter Panel

Controls:

- Device: `AGILENT_34401A` or `KEYSIGHT_34411A`
- Resource
- Refresh
- NPLC
- Connect
- Disconnect

`NPLC` maps to `timing.nplc` and is sent to the DMM measurement configuration.

#### Measurement Panel

Controls:

- Mode
- Sweep
- Start
- End
- Step
- Wait time
- Average count

Start, End, and Step always have explicit units. The available unit list changes
with the selected mode.

`Wait time` maps to `timing.settle_s`. It is the delay after each source target
is applied and before reading the meter.

`Average count` maps to `timing.average_count`. The DMM is read repeatedly and
the arithmetic mean is used as the point value.

#### Output Panel

Controls:

- Directory
- Browse
- File
- Auto suffix

When Auto suffix is enabled, timestamps are appended to the output filename.
CSV rows are flushed point-by-point, so stopped runs retain measured data.

#### Run Panel

Controls:

- Start
- Stop
- Output Off

Start requires Source and Meter to be connected. Stop requests a graceful stop;
the measurement core ramps to zero and turns output off.

Output Off immediately sets source output off, level zero, and output off again.

### Safety Behavior

Normal finish:

1. Ramp source to zero using the configured ramp step.
2. Wait `post_zero_delay_s`.
3. Turn output off.
4. Leave instruments connected and remote.

Stop:

1. Stop at the next interruptible point.
2. Ramp source to zero.
3. Turn output off.
4. Leave instruments connected and remote.

Error:

1. Prefer output off.
2. Report the error to the caller or GUI.

GUI close, Source Disconnect, or All Disconnect:

1. Try output off and zero.
2. Return connected devices to local.
3. Close VISA resources.

The 34401A does not receive `SYST:LOC`; local return is handled through GPIB GTL
to avoid the 34401A RS-232-only command error.

### CSV Output

CSV columns are defined in `src/kohdalab_iv/api/measurement_rows.py`.

The CSV is intentionally compact. Internal measurement rows keep additional
metadata, while the saved file focuses on values needed for plotting and
analysis.

Saved fields:

- `timestamp`
- `elapsed_s`
- `point_index`
- `direction`
- `target_value`
- `target_unit`
- `voltage_V`
- `current_A`
- `resistance_Ohm`
- `conductance_S`

Resistance and conductance are saved to CSV as SI values. The GUI plot header
shows the same quantities with readable engineering units.

`direction` is based on source-target change, not pattern leg names. If the
target increased from the previous point it is `forward`; if it decreased it is
`backward`. The first point uses the direction toward the next point.

### CLI

Check config:

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run kohdalab-iv --config config/default.json check-config
```

List VISA resources:

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run kohdalab-iv list-resources
```

Run measurement:

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run kohdalab-iv --config config/default.json measure iv
```

### GUI

Launch from the Desktop VBS:

```text
Desktop\KohdaLab_IV_GUI.vbs
```

Or launch from the project directory:

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run --extra gui kohdalab-iv-gui
```

### Jupyter

```powershell
.\run_jupyter.ps1
```

Example notebook usage:

```python
from pathlib import Path

from kohdalab_iv.api.config import load_config
from kohdalab_iv.api.experiment import Experiment
from kohdalab_iv.api.scan_plan import iv_plan_from_config
from kohdalab_iv.api.notebook import format_point, make_iv_live_update

config = load_config(Path("config/default.json"))
plan = iv_plan_from_config(config)
experiment = Experiment(config, auto_connect=False)

experiment.connect_device(plan.source_ref)
experiment.connect_device(plan.measure_ref)

live = make_iv_live_update(x_key="current_A", y_key="voltage_V")
rows = experiment.run_iv(
    plan=plan,
    on_point=lambda point: (print(format_point(point)), live(point)),
)
```

### Implementation Layers

```text
apps / CLI / notebooks
  -> kohdalab_iv.api
  -> kohdalab_iv.interfaces
  -> kohdalab_iv.instruments
```

`instruments`

- Low-level SCPI/VISA commands and model quirks

`api`

- config loading
- unit parsing
- scan-plan generation
- device session management
- measurement execution
- CSV row generation

`apps`

- GUI entry point and presentation behavior

### Validation Commands

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run --group dev pytest
& "$env:USERPROFILE\.local\bin\uv.exe" run --extra gui python -c "from kohdalab_iv.apps.iv_gui import main; print('iv_gui import OK')"
& "$env:USERPROFILE\.local\bin\uv.exe" run kohdalab-iv --config config/default.json check-config
```
