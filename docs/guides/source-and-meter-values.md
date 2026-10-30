# Source settings and measured V/I / Source設定値と実測値

KohdaLab IV 1.3.0-alpha.21 separates programmed Source quantities from optional
same-quantity Meter readings. This guide describes the released software
contract. It does not claim a new physical-device qualification.

## Instrument assignment

Choose the Source and **Meter (V)** / **Meter (I)** in Settings. Add/remove
instruments in the left panel as before. One physical instrument cannot occupy
both Meter roles. Changing quantities with two Meters requires disconnecting
both before rebinding; a single Meter retains the existing mode-switch workflow.

| Mode | Required | Optional |
| --- | --- | --- |
| V-I | Voltage Source + current Meter | Voltage Meter across the intended voltage-sense terminals |
| I-V | Current Source + voltage Meter | Current Meter in the intended current path |
| Dual V-I | Two voltage Sources + current Meter | Differential voltage Meter across the intended sense terminals |

The optional Meter measures the same physical quantity as the Source generates.
It has its own Auto/fixed Range control based on its instrument profile. The
existing opposite-quantity Meter limit and Source compliance remain independent.
Readings from two instruments are sequential, not hardware-synchronized; raw
records retain host timing for each physical exchange. Repeat averages each
Meter's accepted conversions at the fixed source target.

## One plot, two axes

The lower x axis is **V_source** (or **I_source**); the upper x axis is
**V_meter** (or **I_meter**). Both use the same actual SI coordinates and scale.
Separate traces overlay Source-based and Meter-based points; no normalization or
assumed linear conversion hides differences. Forward/backward colors remain.
The top axis remains reserved when unassigned, so assigning/removing the optional
Meter does not move or resize the graph. No measured trace is fabricated when
there is no optional Meter.

Source and Meter R/G displays use independent sweep slope fits, as the existing
resistance display did. They are distinct from the per-point V/I and I/V ratios
in CSV. A missing measured pair or a zero/undefined denominator is shown as blank
in CSV (a dash in the GUI), not as zero or infinity.

## Saved values

Analysis CSV schema is `analysis_v4`; run metadata schema is 9; GUI settings schema
is 4. Existing columns retain their names; eight explicit columns are appended:

| Column | Meaning |
| --- | --- |
| V_source_V | Programmed Source voltage readback, or requested voltage if unavailable; Dual uses the programmed differential. Blank for a current Source. |
| V_meter_V | Voltage DMM reading only; blank if that Meter is unassigned or no valid conversion was accepted. |
| I_source_A | Programmed current readback, or requested current if unavailable. Blank for voltage Source modes. |
| I_meter_A | Current DMM reading only; blank if that Meter is unassigned or no valid conversion was accepted. |
| R_source_ohm / G_source_S | Per-point ratios using Source level + the opposite-quantity Meter. |
| R_meter_ohm / G_meter_S | Per-point ratios using measured voltage + measured current; blank without both. |

`voltage_V` / `current_A` use the measured pair when both meter readings are
available; otherwise they retain the existing Source-plus-opposite-quantity-Meter
pair. The origin columns distinguish this fallback. Neither `V_meter_V` nor
`I_meter_A` is fabricated from a Source setting.
Finite numeric values use shortest-roundtrip scientific notation in SI units,
without display rounding. Simulation is explicitly flagged and is not physical
measurement evidence. The raw CSV and run JSON remain companions to the analysis
CSV; preserve all three files. No per-source actual voltage is inferred for Dual.

An assigned Meter failure, overload or unsafe input-range transition stops the
measurement and requests verified output OFF through the existing safety domain.
The app never silently replaces a failed reference reading with a Source value.
Already saved valid points survive; an incomplete reference point is not accepted.
A primary-Meter protection point may be retained as existing evidence, with any
unacquired reference fields left blank.

## Python engine example

```python
from kohdalab_iv_native import NativeIV

with NativeIV() as iv:
    iv.connect(source="GPIB0::5::INSTR", meter="GPIB0::7::INSTR")
    iv.connect_reference_meter("GPIB0::8::INSTR", model="AGILENT_34411A")
    config = dict(iv.default_config)
    config["reference_meter"] = {"model": "AGILENT_34411A"}
    config["meter_voltage_range"] = 1.0  # optional voltage Meter in V-I
    # Review source limits, compliance and sweep values before running.
```

Use the bundled Python client that ships with 1.3.0-alpha.21. It accepts both
`analysis_v4` and legacy `analysis_v3`; an older client rejects the new schema
rather than assuming its earlier contract. The external GUI control API supports
`reference_meter` alongside `source`, `source2` and `meter` in device lists.

## 日本語

SettingsでSource・Meter (V)・Meter (I)を割り当てます。V-I／Dualでは電圧Meterが任意、
I-Vでは電流Meterが任意です。ないMeterの実測値はCSVで空欄にし、Source値で埋めません。
2台のMeterの電圧・電流の役割はモード変更でも維持し、接続中の入れ替えは防ぎます。

グラフは1枚のまま、下軸Source・上軸Meterを同じ単位・目盛で表示し、2種類の線を重ねます。
Meterがなくても上軸領域を確保するため、グラフが上下にずれません。
R/GはSource基準とMeter基準を区別します。GUIは掃引全体の傾きによるフィット値、
CSVの追加列は各点のV/I・I/Vです。CSVはSI単位の指数表記で表示桁へ丸めず保存します。

2台の取得は順次処理で、同時取得ではありません。割り当てたMeterの異常時は黙って
Source値へ代替せず、安全停止・出力OFFの処理を行います。実機での新たな検証は未実施です。
