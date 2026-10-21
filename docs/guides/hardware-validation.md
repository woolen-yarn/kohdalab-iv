# Hardware validation guide

Use this checklist after changing the application, VISA runtime, cable, adapter,
or resource string. Read the instrument manuals and [safety notes](../../SAFETY.md).
New preview instrument models remain physically unverified until qualified on
that exact setup.

## Verify the software path

Install or extract the complete package, including its helper programs. From
its executable directory, test without hardware:

```powershell
.\kohdalab-iv.exe --measure --simulate --output "$HOME\iv-simulation.csv"
```

Confirm that the run completes and writes CSV. Source builds can use
`cargo run -p kohdalab-iv --bin kohdalab-iv -- --measure --simulate --output /tmp/iv-simulation.csv`.
This simulation does not qualify physical communication or measurement accuracy.

## Prepare the physical setup

1. Confirm wiring, polarity, grounding, sample limits, and source mode.
2. Provide a manual way to disable output. Protect or disconnect sensitive samples
   for the first communication check.
3. If using VISA, verify discovery in the vendor connection utility first.
4. Launch the GUI and use Refresh in each device card. Check source and meter
   model and resource independently; see [supported hardware](../reference/hardware.md).
5. Connect each device. Verify identity, zero source level, and output off before
   attaching the sample.

## Review and run a small sweep

In the GUI, select the source mode and a conservative range, compliance, ramp,
settling time, NPLC, and output directory. Review Timing / Compliance before
Start. Keep the first sweep as small as the sample and instrument allow.

1. Watch the source front panel and check measured sign and approximate magnitude.
2. Press Stop and verify the controlled return to zero and output off.
3. Check that the partial CSV is preserved.
4. Run the short sweep to completion, then All Disconnect; confirm local control.
5. Save the reviewed GUI configuration to a local file outside Git.

## Record the result

Record application version, date, OS, VISA runtime, adapter, resources, instrument
firmware, range, compliance, and observed cleanup. Do not publish private lab
settings in packaged defaults. If cleanup fails, disable output manually,
preserve the log, and resolve the fault before increasing the range.
