# Meter ranging

Use **Meter range** in the left-panel Meter card and choose **Auto** or a **Fixed** range for
Voltage measurement and Current measurement. These selections are independent:
V-I and Dual V-I use the current selection; I-V uses the voltage
selection. Changing mode preserves both selections. Save the configuration to
keep them between sessions. Existing profiles default to Auto.

Choose a fixed range that covers every expected reading in both sweep
directions. The app does not visit a high-output point to estimate a range.
Before enabling the source, it disables meter autoranging, sets the selected
range, and checks the meter's autorange state and range readback. It keeps that
range through ramps, settling, repeated readings, and the complete return sweep.
Idle live acquisition uses the same selection.

A range readback mismatch stops acquisition and requests output OFF; an invalid
reading is not accepted into the analysis CSV. Dual requests both sources OFF.
Existing overload, Source compliance, and Measurement limit protection remains
in effect. A fixed measurement range is **not** a source current limit, nor does
it remove a current meter's burden voltage. Select protection limits separately.

## Auto range transitions

For models with verified range readback, Auto records the initial range before
source ON and checks every conversion, including repeated/settling conversions
and Dual. Known changes within the same documented resistance group are allowed
(for example, 34411A 100 µA to 1 mA, both with a 200 ohm shunt). A transition to
a different resistance group stops and requests output OFF before further
acquisition or evidence writes. The trigger remains in raw evidence and is not
accepted into the analysis CSV. The popup recommends a fixed range and a new
complete sweep. Manual DC live acquisition uses the same guard while the sweep sources
are enabled, including the first ON and held-level edits; ordinary idle
acquisition with sweep outputs OFF may autorange freely. A separate fixed gate
bias retains its existing independent hold policy.

If unchanged electrical loading is not established (including voltage input
impedance that depends on High-Z settings and the 34465A's unspecified exact
shunt values), any observed Auto range change stops conservatively. ADCMT 7461A
has no verified range-readback command in this app, so its Auto display clearly
states that transition monitoring is unavailable. Its documented terminal
resistance values are retained as specification data, without enabling unverified
range commands or pretending its Auto transitions can be guarded.

Detection occurs **after a conversion**: Auto can briefly switch the physical
input circuit before software sees the change. Only a fixed range prevents
measurement-driven autoranging in the first place. An unchanged shunt does not
guarantee zero switching transient or include all fuse/lead/contact resistance.

## Limits and layout

The left-panel Source card shows **Compliance**. The Meter card shows
**Limit** and **Range**, each as a compact label/input/unit row above its live display. **Wait time** is in
Settings → Timing. The central Measurement area retains its two-column layout:
sweep controls on the left and Output/Run on the right. Device assignments,
Dual split and less common acquisition settings stay in Settings. Fixed current ranges show verified shunt
or terminal input resistance in the range menu and tooltip, distinguished by
name; unknown values are labeled as unverified there. No permanent explanatory
text sits below Range. Voltage input impedance depends on the instrument's setting.

The units and range presets follow the active voltage/current function. Auto is labeled simply **Auto**; the live channel and limit unit identify the
measured quantity. An open primary DC editor and a held DC output use
the DC function; a running sweep uses its own mode. DC compliance uses the
DC editor's value and is read-only while ON; it is not a second editable copy.
Changing an idle DC function updates live channels only after verified OFF and
never enables output. Closing DC or completing a sweep reconciles idle live
channels with the selected context. The Meter Limit remains a sweep software
threshold, not a replacement for DC hardware compliance.

Meter limit is an independent software stop threshold, not a range selector or
current limiter. If it exceeds a fixed range, the range tooltip explains that
instrument overload may stop acquisition first. The app does not silently alter
either setting. Editing Meter limit preserves the existing Source compliance,
including when loading legacy profiles with coupled defaults.

## Supported presets

| Meter | Fixed DC voltage ranges | Fixed DC current ranges |
| --- | --- | --- |
| Agilent 34401A | 100 mV, 1 V, 10 V, 100 V, 1000 V | 10 mA, 100 mA, 1 A, 3 A |
| Agilent / Keysight 34411A | 100 mV, 1 V, 10 V, 100 V, 1000 V | 100 µA, 1 mA, 10 mA, 100 mA, 1 A, 3 A |
| Keysight 34465A | 100 mV, 1 V, 10 V, 100 V, 1000 V | 1 µA, 10 µA, 100 µA, 1 mA, 10 mA, 100 mA, 1 A, 3 A, 10 A |
| ADCMT 7461A | Auto only | Auto only |

ADCMT fixed-range commands have not been verified against its programming
manual; explicit fixed requests are rejected. Simulation supports fixed ranges
for UI and regression testing. Presets and manual references live in
[`specs/meters.toml`](../../specs/meters.toml). Terminal ratings and instrument
connection requirements still apply; a menu entry is not permission to apply
that voltage or current.

## Python and saved data

Use SI values, or `None` for Auto, in the existing RunConfig:

```python
result = iv.run("fixed-range.csv", {
    "meter_current_range": 0.001,  # fixed 1 mA for V-I
    "meter_voltage_range": 1.0,    # fixed 1 V for I-V / Dual
    "start": -0.001, "stop": 0.001, "step": 0.0001,
    "pattern": "round_trip",
})
```

The Python client rejects explicit fixed requests sent to an older engine or
GUI that cannot honor them. Requested ranges are stored in the `.run.json`
configuration; actual range readbacks remain in the existing detailed
acquisition evidence. Analysis and raw CSV column schemas are unchanged.

## Validation

Native mock tests cover fixed V/I return sweeps, pre-output verification,
range drift during repeats and settling, idle live reads, Dual cleanup, and
preservation of existing overload/limit stops. GUI simulation was visually
checked. Hardware and Windows runtime validation of this change is pending.
