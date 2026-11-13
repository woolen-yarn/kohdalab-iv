# Shared GPIB instruments: development setup

This is an explicit development backend, not a published portable feature.
It supports different primary addresses on one native NI/82357B GPIB adapter.
Physical macOS/Windows simultaneous-operation validation is still pending.
Do not use it as the sole protection for a connected sample.

## Implemented behavior

- One service owns the physical adapter/helper and the existing compatibility
  lock. IV and TRKR connect through separate authenticated local sessions.
- Address ownership is exclusive, including between two instances of one app.
  An app requesting an owned address gets the owner name in its error.
- Complete transfers run on one worker. Deadlines include queue time; expired
  or disconnected queued requests are not replayed later.
- Local protocol v2 uses a single-use nonce from each reply to authorize the
  next request. Pre-pipelined/replayed frames are rejected, including when a
  peer half-closes while a prior command is still awaiting its response.
- App close/REN/EXIT affects that client's session, never the physical helper
  serving other apps. Instrument release uses addressed LOCAL only. Lost
  non-source clients get addressed device-clear to discard pending responses.
- Refresh uses cached listeners while instruments are owned, avoiding bus
  probing/reset during an active measurement. A newly attached address may
  need manual resource entry until all leases are released and Refresh probes.
- Source classification is configured by the operator on the service side;
  clients cannot register a configured Source as an unprotected Instrument.
  Unregistered addresses cannot receive commands.
- Every new source lease verifies the model and output OFF before it is granted,
  including when no old journal exists. **Connecting a source in shared mode
  therefore turns an existing output OFF.** It never restores a previous DC
  setting automatically. The existing cleanup may also zero the programmed
  source level after OFF. This behavior is specific to the new shared backend.
- Active source claims are recorded before clients may write. On lost owner,
  model-specific verified OFF runs before releasing ownership. Failed cleanup
  remains quarantined. A leftover journal after service restart requires
  explicit model verification and OFF recovery, never deleting a lock file.
- A transfer failure faults the controller; clients cannot restart it behind
  another experiment, switch backend, or replay uncertain writes. Existing IV
  safety controls remain active, but recovery requiring a whole-bus reset must
  be coordinated explicitly with all owners.

## Build

From this repository:

```sh
cargo build --release --locked --workspace
```

Build TRKR from its updated source too; an existing published binary does not
have the shared-mode client. The independently licensed native GPIB helper is
still required on the service host. Its redistribution/source obligations are
unchanged, and no firmware download/transfer has been added.

## Register the physical devices

Create `devices.json` in a private configuration directory. Addresses here are
examples; use the actual addresses and models in your setup:

```json
{
  "3": {"kind": "source", "model": "YOKOGAWA_GS210"},
  "5": {"kind": "instrument"},
  "7": {"kind": "instrument"}
}
```

Here address 3 is an IV source, 5 an IV meter and 7 a TRKR lock-in. Register all
output sources as `source` with their correct catalog model; `instrument` is
for non-source instruments such as meters/lock-ins. The device map is trusted
operator configuration and must match the actual wiring. A source that was
physically substituted for a meter must not inherit its old registration.
Change this map only after every app has safely disconnected and the service
has stopped. Register `YOKOGAWA_7651` for that source's legacy cleanup dialect.

## Start one service

Use a dedicated private state directory. Do not reuse an application/project
directory. The service creates new Unix directories with mode 0700 and refuses
an existing group/world-accessible directory. On Windows it replaces the
dedicated directory ACL with a protected current-user-only ACL before publishing
credentials; failure aborts startup. Windows ACL runtime validation is pending.

macOS example (the native helper path depends on your build):

```sh
target/release/kohdalab-instrument-hub --serve \
  --native-helper build/native-gpib/kohdalab-gpib-helper \
  --state-dir "$HOME/.kohdalab/instrument-hub" \
  --devices /absolute/path/to/devices.json
```

PowerShell example:

```powershell
.\kohdalab-instrument-hub.exe --serve `
  --native-helper C:\KohdaLab\support\kohdalab-gpib-helper.exe `
  --state-dir "$env:LOCALAPPDATA\KohdaLab\InstrumentHub" `
  --devices C:\KohdaLab\devices.json
```

Start the service in a shell without `KOHDALAB_INSTRUMENT_HUB_CONFIG`. It
publishes `endpoint.json` in its state directory, containing a loopback address
and a secret session token. Do not copy/share this file or put it in Git.
The service is a foreground development process; keep it running while any
application owns devices. Explicit service startup opens the controller and
caches passive listeners before publishing the ready endpoint. This ensures
IV can still discover its addresses when TRKR connects first. Startup does not
enable source outputs. The service holds the compatibility adapter lock for
its controller lifetime; failed client authentication never opens another helper.

## Connect IV and TRKR

Launch both newly built apps from shells with the same endpoint and client
paths. On macOS:

```sh
export KOHDALAB_INSTRUMENT_HUB_CONFIG="$HOME/.kohdalab/instrument-hub/endpoint.json"
export KOHDALAB_INSTRUMENT_HUB_CLIENT=/absolute/path/to/kohdalab-instrument-hub
# Launch the rebuilt IV / TRKR binary from this shell.
```

On Windows:

```powershell
$env:KOHDALAB_INSTRUMENT_HUB_CONFIG = "$env:LOCALAPPDATA\KohdaLab\InstrumentHub\endpoint.json"
$env:KOHDALAB_INSTRUMENT_HUB_CLIENT = 'C:\KohdaLab\kohdalab-instrument-hub.exe'
# Launch the rebuilt IV / TRKR executable from this PowerShell window.
```

The client executable can alternatively sit next to the native GPIB helper;
the explicit path avoids ambiguity during development. Shared mode takes
precedence over local VISA/native GPIB helper selection. An invalid config,
missing service, incompatible protocol or occupied address produces an error;
there is no local GPIB fallback or driver installation attempt.

Connect IV to its registered Source/Meter addresses and TRKR to its different
registered lock-in address. Their measurement UI and CSV formats stay unchanged.
Stop and disconnect all apps before stopping the service. Source disconnect
verifies OFF. Closing one app does not disconnect another app's instruments.

## Diagnostics and recovery

```sh
kohdalab-instrument-hub --status --config /private/path/endpoint.json
kohdalab-instrument-hub --recover 3 --model YOKOGAWA_GS210 \
  --config /private/path/endpoint.json
```

Recovery explicitly sends source OFF after identity verification. It is refused
while another app owns the address, for a different model, or when the controller
is faulted. For a controller fault, verify outputs at the front panels, stop all
clients and coordinate service restart. Recovery never restores output ON.
Software cannot guarantee hardware output shutdown if the service/OS/USB link
is lost; hardware compliance and independent sample protection still apply.

## Current limits

- One native adapter, GPIB0 primary addresses 1–30. No service-side official VISA
  backend or automatic driver switching; do not replace an existing vendor
  driver just to try this development path.
- No automatic service startup/installer update yet. Existing portable releases
  and legacy apps retain whole-adapter exclusion and cannot share the bus.
- PLE currently uses a separate SDK bridge; Camera uses native USB acquisition.
  Their device ownership adapters are not yet integrated. Both can reuse the
  product-neutral `instrument-protocol` contract when those paths are added;
  camera image streams do not go through the GPIB request queue.
- No same-instrument multi-app observation/readback subscriptions yet; one
  instrument has exactly one owner. Connection loss is observed through the
  local socket. A connected but hung process is not yet heartbeat-expired.
- Safety work has bounded priority but no hard realtime guarantee. An in-flight
  transfer cannot be interrupted safely; a slow device can delay its bus peers.
- Development tests use fake controllers. Hardware and Windows runtime checks
  must precede a coordinated installer/portable release.

See the [cross-application design](../development/shared-instrument-service.md)
for the wider migration and acceptance criteria.
