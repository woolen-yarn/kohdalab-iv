# Legacy instrument port (development)

This port uses the working Python drivers in git revision `b754cfa^` as its protocol baseline. It does not restore the old Python measurement application. The single Rust engine serves the desktop, CLI and optional Python/Notebook client.

## Implementation and validation

- GS210 / 34411A retain their existing command sequences.
- 7651 identifies with `OS;E`, drains the panel response through `END`, and uses `F1` / `F5`, integer range codes, `LA` in mA and `LV` in V. Programmed levels stay in SI units. `OC;E` bit 4 verifies output state; output OFF is sent before zeroing. It never receives GS210 SCPI output commands.
- 34401A uses its legacy SCPI configuration and GPIB local control; no `SYST:LOC` is sent.
- 34465A uses the 34411A-family SCPI sequence and reported range/resolution. Faster NPLC values can require options or firmware; a configuration error stops the run.
- 7461A GPIB requires SCPI command mode. Requested NPLC maps to FAST (<=0.02), MED (<=0.2), SLOW (<=1) or SSLOW; this is a sampling-rate policy, not confirmation of exact requested integration time. USB requires official Windows 64-bit VISA and ADC commands, with `H0`, `F1`/`F5`, `R0`, `ITP`, delayed raw stream reads, and delayed `ERR?` readback. A reading is discarded after each source change before accepting the next sample. Unknown measurement resolution stays unknown; no panel precision is invented.

Model identity is checked before configuration. The connected models must agree with the run models. Range, source resolution, hardware limit, software limit, meter input limit and NPLC are checked before creating the run and enabling output. Legacy source range codes are kept distinct from physical SI ranges when choosing ramp increments.

Protocol fixture tests cover model mismatch, 7651 limit units, model/range queries, output on/off verification and zeroing, ADC SCPI setup, USB ADC setup, delayed status reads, stream read parsing, and discarded conversions. They do not establish physical output accuracy, front-panel parity, cable behavior or every firmware variant.

**Physical validation of the newly ported models is pending.** The released v1.2.0 artifacts are unchanged. GS200 has no separate historical driver and is not newly enabled. 34401A/7651 use GPIB; 34465A USBTMC support is a new extension validated by transport/setup tests only. Native WinUSB access to ADCMT USB is not implemented.

## Automation

Connection messages accept `source_model` and `meter_model`; per-role messages accept `model`. Missing fields retain GS210/34411A defaults for old clients. Run settings carry the same model keys. The Python client forwards its chosen models into runs.

```python
with NativeIV(executable="/path/to/development/iv-cli") as iv:
    iv.connect("GPIB0::3::INSTR", "GPIB0::7::INSTR",
               source_model="YOKOGAWA_7651", meter_model="AGILENT_34401A")
```

Close any GUI holding those devices first. Connection enables monitoring, not source output. Choose permitted limits and verify hardware configuration before a real sweep.

CLI identification and measurement accept `--source-model YOKOGAWA_7651 --meter-model AGILENT_34401A` with the normal resource arguments. Identification still performs safe cleanup before releasing the devices.
