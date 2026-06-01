# KohdaLab IV

## 日本語

KohdaLab IV は、横河 GS210 を source、Agilent/HP 34401A または Agilent
34411A を meter として使う DC IV/VI 測定ツールです。GUI、CLI、Jupyter
Notebook から同じ測定コアを使えます。

現在の標準構成は次の通りです。

- Source: `YOKOGAWA_GS210` at `GPIB0::2::INSTR`
- Meter: `AGILENT_34401A` at `GPIB0::26::INSTR`
- Config: `config/default.json`

### 起動

デスクトップの VBS ランチャーから GUI を起動します。

```text
Desktop\KohdaLab_IV_GUI.vbs
```

プロジェクトディレクトリから起動する場合は次を使います。

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run --extra gui kohdalab-iv-gui
```

測定せずに config だけ確認する場合:

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run kohdalab-iv --config config/default.json check-config
```

Jupyter を起動する場合:

```powershell
.\run_jupyter.ps1
```

### GUI 構成

GUI は 3 パネル構成です。

- 左: `Config`, `Source`, `Meter`
- 中央: `Measurement`, `Output`, `Run`, live plot
- 右: log と折りたたみ可能な最新行の `Field / Value`

`Config` は `config/default.json` を読み書きします。アプリは最後に開いた
config を記憶しますが、このプロジェクトの標準 config は `default.json`
一本です。

`Source` は GS210 を対象にしています。`Meter` では 34401A または 34411A
を選び、DMM の積分条件として `NPLC` を設定します。

`Measurement` では sweep と点ごとの測定条件を設定します。

- `Mode`: `I-V` は電流を流して電圧を測定、`V-I` は電圧をかけて電流を測定
- `Sweep`: `linear`, `round_trip`, `zero_centered`, `custom_list`
- `Start`, `End`, `Step`: unit 付き
- `Wait time`
- `Average count`

plot は sweep している量を x 軸にします。source target が増える点は
`forward`、減る点は `backward` として別色で表示します。抵抗値は実測の電圧と電流から計算し、`mOhm`,
`Ohm`, `kOhm`, `MOhm` などの読みやすい単位で表示します。
コンダクタンスも同様に plot 上部へ `mS`, `uS`, `nS` などで表示します。

### 安全動作

通常終了と Stop:

1. Source を 0 に ramp します。
2. Source output を off にします。
3. 装置は connected/remote のままにして、GUI から次の操作を続けられるようにします。

All Disconnect、Source Disconnect、アプリ終了:

1. Source output off と level zero を試します。
2. 装置を local に戻します。
3. VISA resource を close します。

測定エラー時は、まず source output off を優先します。

### Config

標準 config はこれだけです。

```text
config/default.json
```

測定値は engineering unit で書けます。

- Voltage: `nV`, `uV`, `mV`, `V`
- Current: `pA`, `nA`, `uA`, `mA`, `A`

内部の scan plan と CSV は、解析しやすいように SI 単位の値を使います。

### CSV

CSV は測定中に 1 点ずつ書き込まれるので、途中で Stop してもそこまでのデータが残ります。
出力先とファイル名は GUI の Output パネル、または `config/default.json` の
`measurements.iv.output` で設定します。

主な出力項目:

- timestamp と elapsed time
- point number と direction
- target value と unit
- 読み取った `voltage_V/current_A`
- `resistance_Ohm/conductance_S`

詳細仕様は [docs/iv_measurement_spec.md](docs/iv_measurement_spec.md) を参照してください。

---

## English

KohdaLab IV is a DC IV/VI measurement tool for a lab setup using a Yokogawa
GS210 as the source and an Agilent/HP 34401A or Agilent 34411A as the meter.
The same measurement core is available from the GUI, CLI, and Jupyter notebooks.

The current standard hardware profile is:

- Source: `YOKOGAWA_GS210` at `GPIB0::2::INSTR`
- Meter: `AGILENT_34401A` at `GPIB0::26::INSTR`
- Config: `config/default.json`

### Quick Start

Launch the GUI from the desktop VBS launcher:

```text
Desktop\KohdaLab_IV_GUI.vbs
```

Or launch it from the project directory:

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run --extra gui kohdalab-iv-gui
```

Check the active config without running a measurement:

```powershell
& "$env:USERPROFILE\.local\bin\uv.exe" run kohdalab-iv --config config/default.json check-config
```

Start Jupyter:

```powershell
.\run_jupyter.ps1
```

### GUI Layout

The GUI uses a three-panel layout.

- Left: `Config`, `Source`, `Meter`
- Center: `Measurement`, `Output`, `Run`, and the live plot
- Right: log and collapsible latest-row `Field / Value`

`Config` loads and saves `config/default.json`. The app remembers the last
opened config, but this project keeps one standard config: `default.json`.

`Source` targets the GS210. `Meter` selects either 34401A or 34411A and exposes
the DMM integration setting, `NPLC`.

`Measurement` owns the sweep shape and point timing.

- `Mode`: `I-V` sources current and measures voltage; `V-I` sources voltage and
  measures current
- `Sweep`: `linear`, `round_trip`, `zero_centered`, or `custom_list`
- `Start`, `End`, `Step` with units
- `Wait time`
- `Average count`

The plot uses the swept quantity on the x-axis. Points where the source target
increases are `forward`; points where it decreases are `backward`. They are drawn in different colors. Resistance is calculated from the
measured voltage/current values and displayed with readable units such as
`mOhm`, `Ohm`, `kOhm`, and `MOhm`.
Conductance is also shown above the plot with readable units such as `mS`,
`uS`, and `nS`.

### Safety Behavior

Normal finish and Stop:

1. Ramp the source back to zero.
2. Turn source output off.
3. Keep instruments connected and remote so the GUI can continue.

All Disconnect, Source Disconnect, or app close:

1. Try source output off and level zero.
2. Return instruments to local.
3. Close the VISA resources.

On measurement error, source output off is the first priority.

### Config

The single standard config is:

```text
config/default.json
```

Measurement values can be written with engineering units.

- Voltage: `nV`, `uV`, `mV`, `V`
- Current: `pA`, `nA`, `uA`, `mA`, `A`

Internally, scan plans and CSV files use SI values for analysis consistency.

### CSV

Rows are written continuously during measurement, so partial data remains when a
run is stopped. Output settings are controlled by the GUI Output panel or
`measurements.iv.output` in `config/default.json`.

Key output fields:

- timestamp and elapsed time
- point number and direction
- target value and unit
- read `voltage_V/current_A`
- `resistance_Ohm/conductance_S`

See [docs/iv_measurement_spec.md](docs/iv_measurement_spec.md) for the full
implementation specification.
