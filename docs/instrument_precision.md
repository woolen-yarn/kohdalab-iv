# Instrument precision and display policy audit

Reviewed on 2026-10-14. The authoritative per-model configuration is in
`next/specs/sources.toml` and
`next/specs/meters.toml`. Each model now has a
`display_precision` table with provenance, policy, review status and limitations.
Rust embeds those files; its setting-resolution table and display query
commands are no longer independently hardcoded. Parsing is cached.

## What the numbers mean

Keep these four concepts separate: source setting increments, configured
measurement resolution, measurement uncertainty/accuracy, and panel display
format. Neither advertised digit counts nor the length of a SCPI response
establishes measurement uncertainty. A resolution-based display is a conservative
formatting policy, not a guarantee of an identical front-panel string.

The native display rounds to a decimal place no finer than the available
resolution. For example, a resolution of 1.5 nA leads to the 10 nA decimal place;
it does not mean a 1.5 nA quantization lattice. Zero displays without a negative
sign. Unknown resolution is explicitly marked and does not borrow another
model's specification. Plot coordinates, fitting, protection comparisons and CSV
values retain the original floating-point values. Native CSV uses shortest-round-trip
scientific notation to preserve finite f64 values; this is numerical
persistence, not a claim of 17 physically meaningful digits.

## Reviewed models

| Model | Resolution policy | Evidence and caveats | Native desktop |
| --- | --- | --- | --- |
| Yokogawa GS210 | Match the queried source range to the existing TOML voltage/current tables | Source-setting increments verified against manual section 5.2; programmed level readback is not an independent measured output | Enabled; precision-query hardware check pending |
| Yokogawa 7651 | Match legacy range codes to its own TOML tables | All source increments agree with official specifications; range codes must not be treated as GS210 SCPI range values | Not enabled |
| Agilent 34401A | Read current DC range and configured resolution | Manual documents function-specific resolution queries in V/A and NPLC settings; panel formatting still separate | Not enabled |
| Agilent / Keysight 34411A | Read DC range and configured resolution after each reading | Shared SCPI policy for both brands; NPLC and autoranging affect resolution; queried resolution is not certified panel formatting | Enabled; precision-query hardware check pending |
| Keysight 34465A | Model-specific SCPI range/resolution policy | Integration-time capabilities depend on DIG option/firmware; existing legacy `nplc_values` must not be interpreted as universally available | Not enabled |
| ADCMT 7461A | Unknown until its programming protocol is reviewed | Official product specifications give best-case values and integration dependence; no assumed Keysight-compatible resolution query | Not enabled |
| Simulated source / meter | Explicit illustrative TOML increments | No hardware or uncertainty claim; simulator numbers are not real-instrument fallback values | Enabled |

### GS210

Voltage range / setting increment: 10 mV / 100 nV; 100 mV / 1 µV;
1 V / 10 µV; 10 V / 100 µV; 30 V / 1 mV.
Current range / setting increment: 1 mA / 10 nA; 10 mA / 100 nA;
100 mA / 1 µA; 200 mA / 1 µA. Protection-limit increments are separate.
These are output-setting increments, not the resolution of the optional /MON
measurement. The native engine reads the programmed level, not /MON.
Source: [GS200/GS210 user manual, section 5.2](https://www.yokogawa.com/pdf/provide/E/GW/IM/0000023941/0/IMGS210-01EN.pdf).

### 7651

Voltage increments match the GS210 list above, but the 7651 has its own range
codes (2–6), not the GS210 numeric range commands. Current ranges are 1, 10 and
100 mA with 10 nA, 100 nA and 1 µA increments; there is no GS210-style 200 mA
range. Source: [official 7651 specifications, page 1](https://cdn.tmi.yokogawa.com/7651spec.pdf).

### 34401A

The manual describes resolution in measurement units and supports DC NPLC
settings 0.02, 0.2, 1, 10 and 100. Query the selected function instead of deriving
an absolute current increment from an advertised 6½-digit capability.
Source: [34401A user guide, Measurement Configuration Commands](https://www.keysight.com/us/en/assets/9018-01063/user-manuals/9018-01063.pdf).

### 34411A: Agilent and Keysight

The reviewed commands are `SENS:VOLT:DC:RANG?`, `SENS:VOLT:DC:RES?`,
`SENS:CURR:DC:RANG?` and `SENS:CURR:DC:RES?`. Querying is read-only and must
follow `READ?` when autoranging is enabled. The engine queries after each sample;
when averaging, display precision uses the coarsest encountered resolution and
never adds digits merely because the average count increased.
Sources: [SCPI comparison, SENSE subsystem](https://www.keysight.com/us/en/assets/7018-02044/white-papers-archived/5990-3515.pdf)
and [34410A/34411A data sheet](https://www.keysight.com.cn/cn/zh/assets/7018-01326/data-sheets/5989-3738.pdf).

The data sheet's integration-time table defines resolution using typical
10 V DC RMS noise. Applying that ppm table indiscriminately to every current
range, or calling it a front-panel digit table, would be incorrect. Consequently,
the profile records the read-only configured-resolution queries and does not
invent a universal NPLC-to-display-digit table. Literal panel matching remains
unverified. The two metadata queries add instrument communication per sample;
correct autorange metadata takes priority over a speculative cached range.

### 34465A

Treat this as a separate profile, not a renamed 34411A. The official guide lists
integration-time settings dependent on model options; faster settings are not
unconditional. Native support is still disabled. Sources:
[Truevolt operating/service guide](https://www.keysight.com/us/en/assets/9018-03876/service-manuals/9018-03876.pdf)
and [Truevolt data sheet](https://www.keysight.com/gb/en/assets/7018-03846/data-sheets/5991-1983.pdf).

### ADCMT 7461A

The official product information gives best-case DCV resolution 100 nV and DCI
resolution 1 nA, but describes reduced display digits at high sampling speeds.
Those minima are not universal increments. Its programming manual download
requires a form; the command-level review is therefore explicitly pending.
The profile records no speculative `RES?` query or universal fixed digit count.
Source: [ADC 7461A product specifications](https://www.adcmt.com/en/products/dmm/7451).

## Validation and outstanding work

Tests cover the shared source tables, integer legacy range codes, both 34411A
brand profiles, current/voltage query selection, unknown-device behavior and
simulation-only policies. Existing tests preserve unrounded CSV values when
display metadata changes. Prior mock-instrument checks covered autoranging,
averaging, cancellation and verified output-off. Those checks are not hardware
validation or calibration.

Before claiming literal front-panel matching: compare queried ranges/resolutions
and the actual front-panel display on supported firmware at several NPLC/range
settings. For the ADCMT, first review its official programming manual. Do not
silently enable unsupported native devices because metadata has been documented.
