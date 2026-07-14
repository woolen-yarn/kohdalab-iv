# Hardware Smoke-Test Guide

Use this checklist after installing or changing a VISA runtime, cable, adapter, instrument driver, resource string, or instrument controller. It does not replace the instrument manuals or [Safety notes](../SAFETY.md).

## 1. Prepare a Local Configuration

Copy the packaged development config to a location that is not tracked by Git:

```powershell
New-Item -ItemType Directory -Force "$HOME\KohdaLab\config"
Copy-Item src\kohdalab_iv\resources\default.json "$HOME\KohdaLab\config\iv.local.json"
```

Update:

- VISA resource strings;
- source and meter models;
- source range and `max_abs_source`;
- compliance;
- ramp step;
- NPLC and timing.

Start with the smallest practical absolute source target and conservative compliance.

## 2. Verify the Software Path Without Hardware

```powershell
uv run kohdalab-iv --config src/kohdalab_iv/resources/simulated.json check-config
uv run kohdalab-iv --config src/kohdalab_iv/resources/simulated.json measure
```

Confirm a CSV is written and the simulated run finishes normally.

## 3. Confirm VISA Discovery

Use the vendor connection utility first, then:

```powershell
uv run kohdalab-iv list-resources
```

Every resource used by the local config should appear. Resolve duplicate VISA installations or driver errors before continuing.

## 4. Run Configuration Preflight

```powershell
uv run kohdalab-iv --config "$HOME\KohdaLab\config\iv.local.json" check-config
```

Read the source model, meter model, selected source range, point count, and output path. Do not continue if any value is unexpected.

## 5. Inspect the Physical Setup

- Confirm wiring, polarity, grounding, and sample limits.
- Confirm voltage-source versus current-source mode.
- Confirm a manual way to disable source output.
- Confirm the front-panel range and remote/local state.
- Disconnect or protect sensitive samples during the first communications check.

## 6. Connect from the GUI

Start the GUI with the local config selected:

```powershell
$env:KOHDALAB_IV_CONFIG = "$HOME\KohdaLab\config\iv.local.json"
uv run --extra gui kohdalab-iv-gui
```

Connect the source and meter separately. Verify identification, connection status, source level zero, and output off before attaching a sample.

## 7. Run the Smallest Sweep

Use three points around zero where practical:

1. run `Check` again;
2. start the sweep while watching the source front panel;
3. confirm measured sign and approximate magnitude;
4. press Stop once and verify a controlled zero ramp;
5. confirm output off and partial CSV preservation;
6. run the short sweep to normal completion;
7. use All Disconnect and confirm instruments return to local.

## 8. Record the Result

Record the date, OS, VISA runtime, adapter, resource strings, instrument firmware or command language, tested range, compliance, and observed cleanup behavior. Do not place machine-specific resource strings or sensitive lab details in the packaged default config.

If any cleanup action fails, disable output manually, disconnect the sample if safe, preserve the log, and do not proceed to a wider range.
