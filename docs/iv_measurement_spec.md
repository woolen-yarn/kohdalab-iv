# KohdaLab IV Measurement Specification / 測定仕様

Version: 1.0

## 日本語

この文書は KohdaLab IV の現行仕様をまとめたものです。KohdaLab IV は、実機を使った DC IV/VI sweep を GUI、CLI、Jupyter Notebook から共通の測定コアで扱うためのプログラムです。

### 対象装置

標準構成:

- Source: Yokogawa GS210, `YOKOGAWA_GS210`
- Source VISA resource: `GPIB0::2::INSTR`
- Meter: Agilent/HP 34401A, `AGILENT_34401A`
- Meter VISA resource: `GPIB0::26::INSTR`
- Optional meter: Agilent 34411A, `AGILENT_34411A`
- Optional meter: Keysight 34411A, `KEYSIGHT_34411A`
- Optional meter: Keysight 34465A, `KEYSIGHT_34465A`
- Optional meter: ADCMT 7461A, `ADCMT_7461A`

装置ごとの capability と hard limit は TOML に置きます。

```text
src/kohdalab_iv/instruments/sources/specs.toml
src/kohdalab_iv/instruments/meters/specs.toml
```

測定ごとの設定は JSON で扱います。標準 config は `config/default.json` です。

```text
config/default.json
```

一時的な smoke config や example config は、標準 workflow として採用する場合以外は増やさない方針です。

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

config には将来用の AC field を残せますが、現在の対応 workflow は software timing の DC sweep です。

### Sweep pattern

GUI では次の pattern を選べます。

- `linear`: start から stop まで一方向に sweep
- `round_trip`: start から stop まで行き、その後 start に戻る
- `zero_centered`: `0 -> +max -> -max -> 0` の点列
- `custom_list`: 明示的な点列用

linear current sweep の例:

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

scan plan 生成時に SI 値へ変換します。CSV は target、読み取った `voltage_V/current_A`、`resistance_Ohm/conductance_S` に絞ります。GUI の抵抗とコンダクタンス表示は、読みやすさのために自動で scale します。

- `120 mOhm`
- `1.25 Ohm`
- `4.7 kOhm`
- `2.1 MOhm`
- `1.2 mS`
- `2.5 uS`

### GUI layout

GUI は 3 パネル構成です。左パネルと右パネルは折りたたみ可能です。

Left panel:

- Config
- Source
- Meter

Center panel:

- Measurement
- Output
- Run
- Plot

Right panel:

- Log
- 最新行の Field/Value table

source target が増える点を `forward`、減る点を `backward` とし、plot では別色で表示します。

#### Config panel

Controls:

- Browse
- Load
- Save
- All Connect
- All Disconnect

GUI は最後に開いた config path を記憶します。記憶した path が存在しない場合は `config/default.json` に fallback します。

#### Source panel

Controls:

- Device: `YOKOGAWA_GS210`
- Resource
- Refresh
- Connect
- Disconnect

Source connect 時には GS210 を安全側に初期化します。具体的には output off、level zero、output off を実行します。

#### Meter panel

Controls:

- Device: `AGILENT_34401A`, `AGILENT_34411A`, `KEYSIGHT_34411A`, `KEYSIGHT_34465A`, or `ADCMT_7461A`
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

Start、End、Step は unit 付きで扱います。選べる unit は mode に応じて変わります。

`Wait time` は `timing.settle_s` に対応します。source target を設定した後、DMM を読む前に待つ時間です。

`Average count` は `timing.average_count` に対応します。DMM を複数回読み、その算術平均を 1 点の値として使います。

ADCMT 7461A は source target の変更直後に過渡的な値が output data に残ることがあるため、`Wait time` の後、採用値を読む前に最初の 1 reading を捨てます。

#### Output panel

Controls:

- Directory
- Browse
- File
- Auto suffix

Auto suffix が有効な場合、出力ファイル名に timestamp を付けます。CSV は 1 点ごとに flush するので、Stop した場合も測定済みデータが残ります。

#### Run panel

Controls:

- Start
- Stop
- Output Off

Start には Source と Meter が接続済みである必要があります。Stop は安全な停止を要求し、測定コア側で 0 への ramp と output off を行います。

Output Off は source output off、level zero、source output off をすぐに実行します。

### 接続と session 管理

GUI の Connect は、既に同じ model/resource の開いた device がある場合はその handle を再利用します。古い handle が閉じていたり、VISA の session handle が無効になっていたりする場合は、session map から外して再接続します。

Device Disconnect は該当 device を local に戻して VISA resource を close します。GPIB device の場合は、個別 disconnect でも最後に GPIB board の REN release を試します。ResourceManager は共有されることがあるため、個別 device の disconnect では閉じません。

All Disconnect は接続中の device をすべて close したあと、GPIB board ごとに REN release を試します。GPIB の local 復帰では GTL と REN deassert を組み合わせます。

### 安全動作

通常終了:

1. 設定された ramp step で source を 0 に戻します。
2. `post_zero_delay_s` だけ待ちます。
3. output off します。
4. 装置は connected/remote のまま残します。

Stop:

1. 次の interrupt 可能な点で停止します。
2. source を 0 に戻します。
3. output off します。
4. 装置は connected/remote のまま残します。

Error:

1. output off を優先します。
2. error を caller または GUI に通知します。

GUI close、Source Disconnect、All Disconnect:

1. output off と zero を試します。
2. 接続中の装置を local に戻します。
3. VISA resource を close します。

34401A には `SYST:LOC` を送りません。34401A の RS-232 専用 command error を避けるため、GPIB GTL で local に戻します。

Agilent/Keysight 34411A と Keysight 34465A は同じ 34411A 系の local sequence を使います。ADCMT 7461A も同じ local release 経路を使います。ADCMT 7461A は timeout などで未完了の測定が残った状態でも復帰しやすいように、local release の前に VISA clear、`:ABORt`、`*CLS` を試します。USB 接続では USBTMC/USB488 の local/REN release、GPIB 接続では GTL/REN release を試します。

ADCMT 7461A は `command_language` で SCPI/ADC を切り替えます。標準 config は `scpi` です。SCPI では測定開始時に `*RST`、`:SENSE:FUNCTION 'VOLTAGE:DC'` または `:SENSE:FUNCTION 'CURRENT:DC'`、必要に応じて `:SENSE:<function>:RANGE:AUTO ON`、`:SENSE:<function>:NPLCYCLES <nplc>` を送り、測定値は `:READ?` で取得します。connect 時の status check は `:SYSTem:ERRor?` を使います。ADC mode を明示した場合だけ、従来通り `F1`/`F5`、`R0`、`ITP<nplc>`、`ERR?` を使います。

### CSV output

CSV column は `src/kohdalab_iv/api/measurement_rows.py` で定義します。

CSV は意図的に compact にしています。内部 row には追加 metadata を残しますが、保存ファイルは plot と解析に必要な値に絞ります。

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

resistance と conductance は CSV に SI 値として保存し、GUI の plot 上部では読みやすい engineering unit で表示します。

`direction` は sweep pattern の往路/復路ではなく、source target の増減で決まります。target が前の点より増えた場合は `forward`、減った場合は `backward` です。最初の点は次の点への移動方向で決めます。

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

VBS から起動:

```text
desktop\KohdaLab_IV_GUI.vbs
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
- Optional meter: Agilent 34411A, `AGILENT_34411A`
- Optional meter: Keysight 34411A, `KEYSIGHT_34411A`
- Optional meter: Keysight 34465A, `KEYSIGHT_34465A`
- Optional meter: ADCMT 7461A, `ADCMT_7461A`

Instrument capability and hard-limit metadata live in TOML files:

```text
src/kohdalab_iv/instruments/sources/specs.toml
src/kohdalab_iv/instruments/meters/specs.toml
```

Per-run settings live in JSON. The standard config is:

```text
config/default.json
```

Temporary smoke or example configs should not be added unless they are
intentionally promoted to the standard workflow.

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

Example `dc_vi` linear current sweep:

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
`resistance_Ohm/conductance_S`. GUI resistance and conductance displays are
scaled for readability:

- `120 mOhm`
- `1.25 Ohm`
- `4.7 kOhm`
- `2.1 MOhm`
- `1.2 mS`
- `2.5 uS`

### GUI Layout

The GUI uses a three-panel layout. The left and right panels are collapsible.

Left panel:

- Config
- Source
- Meter

Center panel:

- Measurement
- Output
- Run
- Plot

Right panel:

- Log
- Latest-row Field/Value table

Points where the source target increases are labeled `forward`; points where it
decreases are labeled `backward`. The plot draws them in different colors.

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

- Device: `AGILENT_34401A`, `AGILENT_34411A`, `KEYSIGHT_34411A`, `KEYSIGHT_34465A`, or `ADCMT_7461A`
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

For the ADCMT 7461A, the first reading after `Wait time` is discarded before the
accepted point value is read. This avoids using a transient value left in the
7461A output stream immediately after the source target changes.

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

### Connection And Session Management

GUI Connect reuses an existing open device when the model and resource are the
same. If a stale or invalid VISA session handle is found, the session map drops
it and reconnects.

Device Disconnect returns that device to local and closes the VISA resource.
For GPIB devices, individual disconnect also attempts a final board-level REN
release. Because a ResourceManager may be shared, individual device disconnect
does not close the ResourceManager.

All Disconnect closes all connected devices and then attempts REN release once
per GPIB board. GPIB local return combines GTL and REN deassert.

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

The Agilent/Keysight 34411A and Keysight 34465A use the same 34411A-family local
sequence. The ADCMT 7461A uses the same local-release path. Before local
release, the ADCMT 7461A also tries VISA clear, `:ABORt`, and `*CLS` so a timed
out or pending measurement is easier to recover from. USB connections use
USBTMC/USB488 local/REN release; GPIB connections use GTL/REN release.

The ADCMT 7461A command set is selected with `command_language`. The standard
config uses `scpi`. SCPI setup sends `*RST`, `:SENSE:FUNCTION 'VOLTAGE:DC'` or
`:SENSE:FUNCTION 'CURRENT:DC'`, optional `:SENSE:<function>:RANGE:AUTO ON`, and
`:SENSE:<function>:NPLCYCLES <nplc>`. Each point is read with `:READ?`, and
connect status uses `:SYSTem:ERRor?`. Only explicit ADC mode uses the legacy
`F1`/`F5`, `R0`, `ITP<nplc>`, and `ERR?` sequence.

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

Launch from the VBS launcher:

```text
desktop\KohdaLab_IV_GUI.vbs
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
