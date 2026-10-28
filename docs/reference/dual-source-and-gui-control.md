# Dual voltage source and control of an open GUI

This feature is in development. It does not change the normal single-source
V-I or I-V workflow. Hardware validation of the new mode is pending.

## Devices and measurement settings

The left panel contains saved Source and Meter entries. Use **+ Source** or
**+ Meter** above the device cards to add choices after disconnecting the active
instruments. Remove unused entries with **Remove** on their inactive cards. In **Settings > Devices**, choose the entries for
Source 1 and Meter, and, in **Dual V-I** mode, Source 2. Initial assignments
follow entry order. Active cards keep their normal colors; unused cards are
dimmed. Unused entries are configuration choices and are not opened by All
Connect. Dimming means outside this IV run; it does not certify physical output
OFF or describe a separate API-controlled gate.
All Connect connects the selected roles only. Disconnect before changing role
bindings or mode. NPLC remains inside the selected Meter's Device section.

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

The analysis CSV retains exactly the existing 18 columns, their order, SI
units (V/A), scientific numeric format and the existing `source_readback` /
`source_setpoint` origin labels. In Dual mode its single `voltage_V` column is
V1 − V2; no per-source columns or ratio columns are added. The raw audit CSV
also retains its existing 18-column `raw_csv_v1` contract. Additional physical
exchanges and structured source events use its existing `details_json` field.

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
show its usual live readings, plot, progress and saved data. Its Start/Stop and
safety controls remain usable. Gate voltage cannot change during an IV run;
a rejected command is not replayed after the run finishes. Successful runs and
ordinary manual IV Stop retain gate bias for the next condition. A stopped
`GuiIV.run()` aborts automation and requests all outputs OFF. `gate_output_off()`
turns it off explicitly; leaving the Python context requests and verifies OFF
before releasing control. A protection/failure stop is raised to Python, not
reported as a successful measurement. Per-run filename/directory overrides leave the GUI's saved output preferences
unchanged. The context does not close the GUI.

The local API binds only to 127.0.0.1 and uses a random per-instance token and
one exclusive Python control lease. Discovery files live in
`~/.kohdalab/control/iv-*.json` (Windows: under the user profile). Unix discovery
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
