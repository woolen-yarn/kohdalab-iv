# GUI control API

The IV GUI owns every instrument connection and the existing Rust measurement
engine. The Python client never opens a second hardware connection when using
`GuiIV`. The headless `NativeIV` client remains available separately.

This preview API is protocol **2**. Read `capabilities()` for the application
version, supported actions and safety policy. The common operation names follow
KohdaLab TRKR: `status`, `config`, `plan`, `output`, `acquire`, `heartbeat`,
`release`, `configure`, `load_config`, `set_plan`, `set_output`, `connect`,
`disconnect`, `start`, `result`, `wait`, and `stop`. IV also exposes source-specific
Gate operations and `output_off`. `wait` is a Python polling helper.

## Attach or launch

```python
from kohdalab_iv_native import GuiIV

# Read-only inspection does not claim control or interrupt a manual measurement.
observer = GuiIV()
print(observer.capabilities())
print(observer.status())
observer.close()

# Launch an independent GUI using a complete saved GUI JSON profile.
with GuiIV.launch(profile="experiment.json") as iv:
    connected = iv.connect()  # Active source(s) and meter from that profile.
    iv.wait(connected["id"])
    job = iv.start(filename="field_0T_temperature_4K")
    result = iv.wait(job["id"])
    print(result["output"])
```

`GuiIV()` discovers a running GUI. When multiple GUIs exist, pass its private
`endpoint=` file explicitly. `launch()` selects the newly launched PID; it does
not attach to another GUI by recency. `--gui-api --config FILE` can also be used
from the command line. An invalid explicit profile or explicit API startup
failure is an error, rather than a fallback to another experiment's settings.

## Operation ownership

The first mutating call, or entering a Python context, acquires an exclusive
lease. Acquisition is rejected during manual operations or open dialogs. Normal
GUI configuration, device connection, output settings, Start and Save inputs are
locked while Python owns control. The existing plots and data remain visible.
A `PYTHON CONTROL` banner provides **Stop** and **Take control**; GUI Stop remains
available independently of the normal API queue.

Take control permanently revokes that client's ownership, requests Output OFF,
and keeps ordinary inputs locked until a fresh engine acknowledgement verifies
all connected outputs OFF. A revoked/expired client must create a new client;
it cannot silently reacquire control. Voluntary `release()` permits reuse of the
same client after safe cleanup. If the engine dies, the GUI reports that OFF
cannot be confirmed and unlocks for diagnosis; restart the application and check
instrument outputs manually before continuing.

Unlike TRKR's continuation policy, IV deliberately keeps its existing source
safety policy: lost heartbeats (10 seconds), client `close()` and `release()`
request Stop/Output OFF. The client verifies OFF before releasing control.
Dropping a client object stops its weak-reference heartbeat, so it cannot retain
ownership indefinitely. The GUI must be responsive for its lease timer to run;
this does not replace hardware compliance or an independent interlock.

Successful IV sweeps and ordinary manual sweep Stop retain an enabled gate bias.
A failed/protected automated run raises an error and requests all outputs OFF.
Gate voltage changes during a sweep are rejected, never deferred until afterward.
`gate_output_off`, `output_off` and Stop use the existing immediate safety path.

## Settings and jobs

- `config()` returns `profile` (the saved GUI settings) and `config_path`.
  `configure(profile)` and `load_config(path)` require disconnected instruments.
- `plan()` returns the current `RunConfig`. Change a copy with `set_plan(plan)`;
  the same exact sweep/range/ratio validation applies as GUI Start. If live mode
  must change, the client waits for the tagged mode-switch result. Changing
  instrument models requires a disconnected profile. Leaving Dual mode requires
  Source 2 to be disconnected, as in the GUI.
- `output()` returns `dir`, `filename`, `auto_timestamp_suffix`.
  `set_output(...)` changes only supplied fields. Per-run `filename`/`directory`
  overrides leave the saved output preferences unchanged.
- `connect(devices=...)` and `disconnect(devices=...)` accept `source`, `source2`,
  `meter` roles and return a job receipt. Each stage completes before the next
  begins; failures and Stop cancel the remaining stages. Gate connections use
  their explicit Gate methods.
- `start()` returns a receipt immediately. `result(id)` is read-only and `wait(id)`
  waits for that exact job. Receipts include `id`, `kind`, `status`, `rows`,
  `output`, `error`, `error_code`, and the final `snapshot`. Status is `running`,
  `completed`, `stopped`, or `error`. Stopped/protected/failed jobs are not
  successful measurements. The legacy `run()` helper returns the final Snapshot;
  `configuration` remains a legacy alias for the current RunConfig.
- `start`, `connect`, `disconnect` accept a caller-provided UUID `request_id`.
  Retrying the identical operation with that UUID returns the same receipt,
  rather than executing twice. Reusing it for another method or payload returns
  `request_id_conflict`. Authentication/lease fields are excluded from the
  fingerprint. Keep the UUID when resolving a lost acknowledgement.
- The last **32** receipts are retained in memory for the GUI session, not across
  application restart. Old IDs cannot be reused after their receipt expires;
  the API rejects additional jobs after 100,000 IDs until a GUI restart. The
  CSV/raw CSV/run JSON remain the on-disk measurement evidence.

## Errors and compatibility

`GuiIVError.code` distinguishes `busy`, `authentication`, `ownership_lost`,
`unknown_job`, `request_id_conflict`, `stopped`, `protection_stop`, `timeout`,
`disconnected` and engine diagnostic categories. English messages explain the
problem. API rejection codes are diagnostic and never decide whether physical
output shutdown is required. `run()` handles timeout/interrupt/error through
Stop and Output OFF; `start`/`wait` permit explicit asynchronous orchestration.
Use a context manager for reliable cleanup.

The wire API accepts `method` and TRKR-style `action` names. Success contains
both `result` and the existing IV `data` envelope. Read-only methods do not require
a lease; mutation requires the instance token and current lease. Protocol 1
clients should be upgraded together with the GUI for the new job/control model.
The local endpoint is loopback-only, uses bounded requests/connections and
private discovery files, and exposes no arbitrary SCPI or shell execution.

Existing sweep algorithms, compliance, hardware limit handling, units and CSV
contracts are unchanged. External conditions remain in caller-selected filenames.

## Developer checks

With a visible desktop and a development build, run:

```sh
python scripts/verify_gui_api.py target/debug/kohdalab-iv --output-dir validation/gui-api
python scripts/verify_gui_control.py target/debug/kohdalab-iv --output-dir validation/gui-control
```

Both scripts use isolated profiles and simulation instruments only. They verify
profile launch/configuration, connection sequencing, exact job lookup and retry,
mode switching, safe release/reacquisition, gate sequencing, lease loss, Stop/OFF,
unchanged CSV columns and GUI rendering. They do not establish hardware validation.

Recorded validation scope and limitations: [GUI API v2 validation](../validation/gui-api-v2.md).
