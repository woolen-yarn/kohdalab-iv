# Safety Notes

KohdaLab IV controls electrical source and measurement instruments. Software
checks are useful, but they are not a substitute for instrument limits,
interlocks, current compliance, correct wiring, and operator supervision.

## Before Measurement

- Confirm wiring and polarity.
- Confirm source mode: voltage source or current source.
- Set conservative source limits and compliance.
- Start with a small sweep range.
- Verify the output file path before long measurements.
- Keep a manual way to disable output.

## During Measurement

- Do not disconnect instruments during an active sweep.
- Use the GUI Stop action or interrupt the CLI so cleanup can run.
- Watch for compliance, overload, unexpected resistance, or unstable readings.
- Stop immediately if the sample, wiring, or instruments behave unexpectedly.

## After Measurement

- Confirm the source returned to zero or output off.
- Save the config used for the run.
- Record any manual hardware changes in lab notes.

The software is provided without warranty. Operators are responsible for safe
laboratory practice and for following instrument manuals.

## Active sweep protection

Active-sweep protection checks every acquired meter reading, including live
readings during ramps and settling. On software compliance, source OFF is
requested before further instrument queries or saving; no normal cleanup ramp
is performed with output enabled. Normal Stop still returns the source to zero
before OFF. Live protection samples are recorded in the run JSON, not inserted
as extra CSV sweep points. An averaged point stopped by compliance identifies
its source value as a commanded setpoint when no pre-OFF readback was obtained.
Before enabling a real source, the engine confirms function, programmed range
(GS210 SI range / 7651 range code) and zero setting; it also confirms output ON.
These are programmed-setting checks, not an external calibration or guarantee
of physical output accuracy. The policy applies to all supported meter models.
Software compliance can only react to acquired samples; retain conservative
instrument hardware compliance and a manual way to disable output.
