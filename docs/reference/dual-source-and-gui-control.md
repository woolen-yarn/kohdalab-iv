# Dual voltage source and control of an open GUI

This feature is in development. It does not change the normal single-source
V-I or I-V workflow. Hardware validation of the new mode is pending.

## Devices and measurement settings

The left panel contains saved Source and Meter entries. Use **+ Add device**
to select a type and enter an optional label. Set the model and resource on
the device card. Labels appear on the cards and in the role selectors and
are saved with the configuration. A newly added device takes over the usual
Source 1 or Meter card when that role is disconnected. A stored device can
also be selected from its Device controls. Remove unused entries with **Remove** on
their inactive cards. In **Settings > Devices**, choose the entries for
Source 1 and Meter, and, in **Dual V-I** mode, Source 2. Initial assignments
follow entry order. Active cards keep their normal colors; unused cards are
dimmed when disconnected. A stored card connects independently and displays
live readback as soon as it connects. A registered source can take over
Source 1 once both source outputs are verified OFF. Switching retains both
connections. All Connect opens the selected roles.

During a sweep, the selected devices keep their live plots and their controls
are locked. Connected unassigned devices keep their own live plots. Their
Source cards can turn fixed voltage/current output on or off and adjust the
level while the sweep is running. Stop shuts down the sweep sources. All Off
also shuts down unassigned sources. If an independent source cannot confirm
OFF after a fault, the engine stops the sweep and attempts recovery. Shared
GPIB commands are serialized on the controller. Change connections and role
assignments between sweeps. Open **Device** on the selected Source for sweep
compliance and the maximum target, or on any Source for manual DC compliance.
The measurement Meter's **Device** section contains its stop limit, range, and NPLC.

測定中は使用機器の操作がロックされ、ライブプロットは表示されます。接続済みの未割り当て
機器は測定中もライブ更新を続け、Sourceカードでは固定DC出力のON・OFFとレベル変更が
できます。Stopは測定用ソースを停止し、All Offは未割り当てSourceにもOFFを要求します。
独立Sourceに異常が発生してOFFを確認できない場合、測定を停止して復旧を試みます。
同じGPIBコントローラへの通信は順番に実行されます。接続と役割の変更は測定間に行います。

The primary Source and Meter live plots begin independently on connection.
The Source plot reports programmed level readback. A sweep still requires
both roles.

Dual mode uses two voltage sources and one current meter. It supports the same
GS210/7651 combinations as the existing source drivers. It does not add dual
current-source operation or simultaneous acquisition from several meters.

Set a positive integer voltage ratio **a:b** in Settings (each 1–1000):

- Source 1: `+a / (a + b) × D`
- Source 2: `−b / (a + b) × D`
- Differential voltage: `D = V1 − V2`

The default 1:1 gives +D/2 and −D/2. A 2:1 ratio at D = 3 mV gives +2 mV and
−1 mV. Ratios other than 1:1 intentionally shift the programmed common-mode
voltage. Symmetric programmed levels do not prove that the actual channel
midpoint is 0 V, because contacts and channel resistance may be asymmetric.
Commands update the sources sequentially; there is no hardware simultaneity.

Planning checks the exact combined source resolution before output is enabled.
An unrepresentable endpoint is rejected; the requested step may be increased
onto the shared grid, as shown in the existing plan preview. Each source has its
own hardware current compliance. A source limiter, overload, communication
failure, or external meter limit stops the run and requests OFF on both IV
sources. An active API-controlled gate also turns off on failure or protection
stop. OFF is reported as verified only when verification succeeds.

### Sweep and gate shutdown / スイープ用ソースとゲートの停止

On normal completion or Stop, the engine ramps the sweep source toward zero
before requesting and verifying Output OFF. In balanced mode, each step
commands both sources in sequence, using the configured ratio, before the next
step. OFF is verified for both sources. Protection trips, communication errors
and emergency OFF requests skip the ramp.

The zero-return budget is 10 seconds. At the cutoff the engine proceeds to OFF
and readback, shows a warning, and saves the last programmed levels in
`zero_return_timeout` in the run JSON and in the run log. The ramp budget
prevents new level writes after the cutoff. An in-flight instrument command
still uses its transport timeout.

A command error, including input validation, requests immediate OFF for a held
gate. If the gate’s `:OUTP?` poll fails while idle, the engine attempts gate
OFF and reports the verified result as `Off` or `Unknown`.

正常終了または通常の Stop では、スイープ用ソースをゼロへ戻してから Output OFF を要求し、
読み戻して確認します。balanced 測定では、設定した比率に従って各ステップで 2 台へ順に指令し、
両方の OFF を確認します。保護作動・通信エラー・緊急 OFF ではゼロ戻しを省略します。

ゼロ戻しの上限は 10 秒です。打ち切った場合も OFF を要求して読み戻し、画面に警告を表示します。
最後に指令したレベルを run JSON の `zero_return_timeout` と run ログに記録します。
打ち切り後は新たなレベルを書き込みません。送信済みの機器コマンドには転送タイムアウトが適用されます。

入力検証を含むコマンドエラーでは、保持中のゲート出力に即時 OFF を要求します。
待機中のゲートの `:OUTP?` が失敗した場合もゲート OFF を試み、読み戻しの結果に応じて
`Off` または `Unknown` を報告します。

### Saved data / 保存データ

Dual V-I automatically plots the voltage meter's signed readings on the
horizontal axis. When no voltage-meter readings are available, it plots the
programmed Source 1 − Source 2 differential.

The Dual analysis CSV uses `analysis_dual_v1`. It retains the 26 shared
`analysis_v4` columns and appends `source1_setpoint_V`,
`source2_setpoint_V`, `source1_level_readback_V`, and
`source2_level_readback_V`. The readback columns are blank when unavailable.
`V_meter_V` is measured voltage and `I_meter_A` is measured current.
`voltage_V` uses `V_meter_V` when present and otherwise uses `V_source_V`,
the programmed differential. `voltage_origin` records that choice.
The raw audit CSV uses `raw_csv_v1`, with source events in `details_json`.

Dual V-Iの横軸は電圧計の実測値を優先し、実測値がないときはSource 1 − Source 2の設定差を使います。
Dualの解析CSVは共通26列に各Sourceの設定値・設定読返しを計4列追加します。読返しがなければ空欄です。
解析には `voltage_V` と `I_meter_A` を使えます。電圧の由来は `voltage_origin` で確認できます。

The companion `.run.json` keeps Dual details separately:

- `balanced_drive.ratio`: the integer weights `[a, b]`.
- `dual_source_points`: one entry per saved analysis-CSV point, joined by
  `point_index`, with timestamp, per-source `setpoint_V` and `level_readback_V`,
  differential level readback and programmed common-mode level readback.
- Each source's `output_voltage_V` is `null` unless independently measured;
  the current drivers do not invent terminal-voltage measurements from source
  setting queries. Unavailable readbacks (including simulation or a limit stop)
  are also `null`.
- Source identities, ranges and hardware compliance remain in the metadata.

Point details are streamed from the durable raw audit into the run JSON on
finalization. They are not accumulated as another in-memory sweep-sized array.
Point counts and indexes must match the saved CSV before finalization; missing or
disordered audit evidence leaves the manifest incomplete and fails the run.
The existing Save action copies the JSON together with the measurement CSV.

## Python controls the running GUI

See the [GUI API reference](gui-api.md) for profile launch, ownership, common TRKR
API names, asynchronous jobs, error codes and compatibility.

Install the thin Python client from `bindings/python`. Open the updated GUI,
select/connect instruments, choose the sweep and save settings normally.
`GuiIV` attaches to that GUI; `NativeIV` remains available for headless use and
starts a separate engine. Do not use separate device owners for one adapter.

```python
from kohdalab_iv_native import GuiIV

with GuiIV() as iv:
    # Explicit resource/model and sample-approved limits are required for real use.
    iv.connect_gate("GPIB0::5::INSTR", model="YOKOGAWA_GS210")
    for gate_v in gate_voltages:  # supplied and approved by your experiment
        iv.set_gate_voltage(gate_v, compliance=gate_current_limit,
                            ramp_step=gate_ramp_step)
        # Wait for external temperature/field stability here if needed.
        result = iv.run(filename=f"gate_{gate_v:+.4e}V", directory=output_directory)
        print(result["path"])
```

External parameters stay in Python and the requested filename. No external
parameter fields or gate controls are added to the GUI. The GUI continues to
show its usual live readings, plot, progress and saved data. During Python control, ordinary inputs are locked; GUI Stop and Take control
remain available. Gate voltage cannot change during an IV run;
a rejected command is not replayed after the run finishes. Successful runs and
ordinary manual IV Stop retain gate bias for the next condition. A stopped
`GuiIV.run()` aborts automation and requests all outputs OFF. `gate_output_off()`
turns it off explicitly; leaving the Python context requests and verifies OFF
before releasing control. A protection/failure stop is raised to Python, not
reported as a successful measurement. Per-run filename/directory overrides leave the GUI's saved output preferences
unchanged. The context does not close the GUI.

The local API binds only to 127.0.0.1 and uses a random per-instance token and
one exclusive Python control lease. Discovery files live in
`~/.kohdalab/iv/run/iv-*.json` (Windows: under the user profile). Unix discovery
files are owner-readable only; Windows uses the user profile's inherited access
controls. Do not share these token files. No arbitrary SCPI or shell command API
is exposed. If several GUIs are open, pass the desired file as
`GuiIV(endpoint=...)`. The client sends periodic heartbeats; after 10 seconds
without its lease heartbeat, the responsive GUI requests Stop and output OFF.
This software timeout does not replace instrument-side compliance or an
independent hardware interlock.

## Developer verification

With a visible desktop and a development build, run:

```sh
python scripts/verify_gui_control.py target/debug/kohdalab-iv --output-dir validation/gui-control
```

This uses an isolated profile and only simulation instruments. It checks lease
expiry/OFF, exclusive control, rejection of gate changes during measurement,
Stop, two completed gate/IV sequences, CSV row counts and GUI rendering. It
also saves a screenshot and a JSON report. Stateful helper mocks and unit tests
cover physical command sequences, both-source OFF ordering, limiter/read
failures, failed secondary OFF, gate recovery and integer ratio planning.
These tests are not substitutes for sample-approved hardware validation.
