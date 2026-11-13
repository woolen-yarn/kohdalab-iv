# Shared instrument service

Status: native GPIB development backend implemented, opt-in only. The shared
protocol/service and IV/TRKR adapters are available in the development sources;
the broader design below is the target, not a claim that all adapters and
packaging have shipped. See [development setup and limitations](../guides/shared-instruments-development.md).
Do not remove the existing adapter lock to enable simultaneous use.

## Scope and evidence

IV, TRKR, PLE and Camera should share an instrument ownership contract.
Sharing a GPIB controller does not mean sharing unrestricted control of an
instrument. The initial target is different instruments on one controller;
multiple applications observing one instrument can use published readbacks.

Local source inspection on 2026-10-29:

| Application | Current path | Consequence |
| --- | --- | --- |
| IV (`80139ed`) | `iv-transport::DeviceLease`, `HelperSession`, and `DevicePair` | Owns the complete GPIB adapter via `~/.kohdalab/iv-next-locks/gpib-adapter.lock`; serializes its own endpoints. |
| TRKR (`066f896`) | `trkr-transport::Gpib`, `acquire_lease()` | Uses the same adapter lock; opening both applications on the same controller is intentionally refused. |
| PLE (`8990b89`, local `kohdalab-pl`) | `bridge/ple_bridge.py::LegacyThorlabsPowerMeter` | The inspected meter adapter loads a user-supplied SDK. It is not an existing KIV GPIB client. Its real transport must be identified before routing it through the service. |
| Camera (`34667f9`) | `camera_lab::session::Worker`, native USB camera acquisition | No GPIB implementation found in the inspected Rust/Python sources. Camera ownership can follow the common contract; frame streaming stays on its existing data path. |

These are local source observations, not multi-application hardware validation.
PLE/Camera have not been modified. IV/TRKR now route through the shared service
only when explicitly configured; their default direct backend is unchanged.

## Ownership boundaries

```text
IV engine ----\
TRKR engine ---+-- authenticated local IPC -- Instrument service
PLE adapter --+                                |
Camera control/                                +-- controller worker -- GPIB helper
                                               +-- device worker ------ USB/SDK

Camera frames: existing acquisition worker -> image processing/display
```

The service alone opens each GPIB adapter. One worker serializes complete bus
transactions, including command plus response for a query. Per-address locks
in separate processes are insufficient: addressing, bus control and USB helper
initialization also affect other addresses. Do not open/close a helper for each
request or use application-level mutexes as a cross-process lock.

Use a canonical physical controller identity (backend plus adapter serial or
verified physical identifier), not just `GPIB0`. A lease identifies the
controller and primary/secondary address; USB uses VID/PID/serial/interface.
VISA and native aliases for the same controller share ownership. Known aliases
for one instrument reached through different interfaces must also be excluded;
ambiguous mappings require explicit configuration rather than guessing.

One application owns instrument commands and configuration. Other applications
receive `ResourceInUse` with the owner name. An optional observer subscription
receives cached measurements, timestamp, unit, provenance and stale state from
the owner; it cannot independently change range, trigger or integration time.
If the owner does not publish a reading, observers report it unavailable.

App-local measurement planning, model validation, compliance checks, output
verification, GUI/API ownership and CSV formats remain in their existing
engines. The service does not replace those safety checks.

## Scheduling and bus lifecycle

- Queue requests with deadlines and bounded frame/queue sizes. A deadline covers
  time waiting as well as execution; expired work is never executed later.
- Treat a query and its response as one transaction. Explicit multi-command
  atomic batches are bounded; never reserve the bus for a whole sweep or during
  user wait/settling time. A batch does not provide hardware-synchronous outputs.
- Use fair scheduling between clients, with bounded priority for safety work.
  Live display requests can be coalesced; output-setting requests are never
  replayed or coalesced into a later unexpected output.
- Stop cancels that client's queued work and advances its verified shutdown.
  It cannot interrupt bytes already in flight; the current bounded transaction
  must finish or retire its response before another transaction begins.
- Never interleave another client's response after a timeout. If the transport
  cannot safely drain/resynchronize, mark the controller faulted, fail its
  pending requests, and require explicit recovery. No retry through a different
  driver/backend and no replay of writes with uncertain completion.
- Choose official VISA or the supported native backend once per controller
  lifetime. Installing/removing a driver is not a response to a busy resource.
- Keep REN asserted as required by active users. Disconnecting one app must not
  send global REN-off, IFC, reset, `EXIT`, or local commands to other clients.
  This requires translating legacy KIV session teardown, not blindly forwarding
  every helper frame. Addressed device-clear/local operations require ownership.
- Refresh uses a service-owned cached inventory. An active session is not reset
  for discovery. New probes are serialized and non-destructive; identity queries
  must not consume or clear another instrument's pending measurement response.

## Safety and process loss

Leases are session-bound, not filenames to delete. A heartbeat detects loss of
the owning app, but timeout alone never grants another app control of a source
that may still be ON. Each output-capable session must register a verified,
model-specific disconnect policy before output can be enabled. A generic
string command must not be assumed to be a safe output-off for every model.

On graceful release, finish the existing owner's cleanup and acknowledge OFF
before releasing an output lease. On app loss, cancel queued work, mark output
unknown, run the registered safety cleanup, and quarantine failed cleanup.
Reconnection must reconcile actual state; it must not restore previous output.
Safety operations that affect an entire bus must notify every affected client.
Failure of one client must not silently turn off another experiment's healthy
gate bias. Physical emergency shutdown is distinct from ordinary app Stop.

For IV specifically, preserve its current difference between sweep Stop,
source-specific DC OFF, and all-output fault cleanup. The independent gate
continues to follow its existing hold/fault policy during healthy ownership.
Do not extend a "hold gate" promise across a lost owner/service connection.

Service restart invalidates every old session token. Outputs are unknown until
verified. An output lease remains blocked until safe reconciliation, even if a
cached service record says the previous output was OFF.

## Local API and packaging

Use the same versioned client contract from all four applications. Required
operations: negotiate, identify client, enumerate, acquire/release instruments,
transact, cancel, publish/subscribe readbacks, heartbeat and owner diagnostics.
Use request/session IDs and stable typed errors (`ResourceInUse`, `Deadline`,
`Cancelled`, `ConnectionLost`, `StateUnknown`, `IncompatibleVersion`). No
unrestricted "reset bus" operation in an ordinary application lease.

Run on demand for the current user, not as an administrator/system service.
Use a per-user Unix domain socket on macOS and an ACL-restricted named pipe on
Windows (or authenticated loopback with equivalently restricted credentials).
Negotiate protocol and backend compatibility before any hardware access. A
failed handshake cannot fall back to independently opening the controller.
Closing one app must not kill the service while another has active leases.

During migration, the service holds the existing IV/TRKR adapter file lock for
its entire controller session. Updated clients connect to the service; legacy
clients fail with their existing busy error rather than resetting the bus.
Do not remove compatibility locks or allow both service and direct controller
access. New installs must ship compatible service/client versions and use one
shared runtime location instead of app-specific copies competing for a device.

Keep native GPL helpers as separate processes with their source/license
distribution obligations. The service neither adds firmware downloads nor
changes redistribution permissions for vendor SDKs.

## Implementation and acceptance sequence

1. Extract a product-neutral Rust protocol/client and build the service with a
   deterministic fake controller. Implement ownership, scheduling, lifecycle,
   safety registration and compatibility fencing before connecting hardware.
2. Route IV and TRKR through it together behind an explicit development backend.
   Validate every legacy session command, particularly `REN`, `LOCAL`, `CLEAR`
   and `EXIT`. Preserve their engine and measurement tests unchanged.
3. Add PLE's actual transport adapter after its SDK contract is checked. Use the
   same ownership API for Camera controls; leave camera frame transfer alone.
4. Validate macOS and Windows, then ship coordinated installers/portable kits.
   Do not advertise coexistence before the following checks pass.

Required checks:

- IV sweeps two sources and a meter while TRKR polls a different GPIB address:
  one hardware open, no interleaved query/read replies, no response theft.
- Same physical resource/alias requested by another app: refused with owner
  details, no command/configuration/output change.
- Repeated Refresh, app close/reopen and one client's disconnect: other clients
  retain connection and REN, without IFC/reset/global local or helper restart.
- Busy/slow device and high-rate live polling: deadlines include queue time;
  bounded fairness and Stop latency are measured, not assumed.
- Pending write cancellation, transport timeout and truncated reply: no stale
  reply consumed by another client, no automatic write replay.
- Owner crash/heartbeat loss, service crash and failed OFF: output state stays
  unknown/quarantined until verified, never silently handed to another app.
- Stop from IV affects the documented IV outputs; unrelated TRKR/PLE work and
  healthy external gate outputs are not implicitly terminated.
- Legacy and new apps together: old exclusive locks still prevent unsafe
  direct access. Incompatible client/service versions fail before USB opens.
- GUI and Notebook APIs, CSV/raw CSV, file naming and safety limits retain
  their existing behavior. No firmware or SDK is newly bundled.

The deterministic service tests and actual IV transport/proxy test run without
physical GPIB devices. Coordinated hardware/release validation remains pending.
Current published direct-helper releases cannot be made concurrent by deleting
a `.lock` file. Keep separate controllers/interfaces or sequential operation
for those releases.
