# Meter ranging

Open **Settings… → Meter range** and choose **Auto** or a **Fixed** range for
Voltage measurement and Current measurement. These selections are independent:
V-I uses the current selection; I-V and Dual voltage drive use the voltage
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
