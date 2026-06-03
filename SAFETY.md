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
