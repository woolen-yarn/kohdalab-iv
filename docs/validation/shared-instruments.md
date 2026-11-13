# Shared instrument service: development validation

Date: 2026-10-29. Host: macOS Apple silicon. All service controller operations
in this record are fake; no physical instrument was opened or energized.
This is source/development validation, not a portable release qualification.

## Implemented paths exercised

- Product-neutral protocol v2 authentication, bounds, response IDs and
  single-use per-reply request nonces. Incompatible/missing nonce fields fail
  closed. A semantic resource conflict keeps other healthy owned devices usable.
- One service with different IV/TRKR addresses, exclusive ownership of an
  address, exact operator-defined source policy, and no client policy downgrade.
- Source model/OFF verification before claim, OFF on lost client, persistent
  quarantine on service restart and explicit verified recovery.
- Queue deadline expiry with no later execution, and a raw socket that sends
  two operations then half-closes while the first fake query is held. The
  second write is rejected before that query is released; the other client
  keeps working.
- Faulted initialization is latched and not retried on another claim.
- Cached startup inventory still exposes IV addresses when TRKR connects first;
  Refresh with leases does not issue another listener probe.
- Actual IV `DevicePair` code through the real stdio proxy and local service,
  including source/meter identity and verified source shutdown.
- Actual updated TRKR `Gpib` code through the same proxy/service, including
  normal query and EXIT teardown while an IV source remains owned.
- Both actual IV and TRKR transport clients running concurrently against one
  fake controller, successfully reading addresses 3/5/7 and exiting. No global
  REN/EXIT reaches the controller on either client's teardown.
- The state-directory guard refuses unrelated data before changing directory
  permissions, and preserves the unrelated file.

The ordinary workspace tests also retain the native/mock sweep, Dual V-I,
manual DC, gate, protection, transport recovery, CSV and GUI contracts. TRKR's
13 pre-existing transport tests passed unchanged. The final IV workspace run
passed 219 tests (three deliberate child/external-probe ignores); both explicit
external-TRKR integration cases were also run successfully. Workspace and TRKR
transport clippy passed with warnings denied, and the release workspace built.

## Reproduce

In IV:

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --release --workspace --locked
```

In the updated TRKR checkout:

```sh
cargo test -p trkr-transport --locked
cargo clippy -p trkr-transport --all-targets --locked -- -D warnings
cargo build -p trkr-transport --example shared_bus_probe --locked
```

The cross-repository test is deliberately ignored in ordinary IV test runs;
IV must not require a sibling TRKR repository to build. To run it explicitly,
set the path to the TRKR example built above (add `.exe` on Windows):

```sh
KOHDALAB_TRKR_SHARED_PROBE=/absolute/trkr/target/debug/examples/shared_bus_probe \
  cargo test -p kohdalab-instrument-hub --test iv_proxy \
  actual_iv_and_trkr_transports_share_controller --locked -- --ignored
```

The test harness itself supplies a temporary fake endpoint and fake-child
marker. Do not set that marker or point the probe at a physical service.
Children have bounded waits and are killed/reaped on a failed assertion;
the fake service is shut down/joined by its fixture.

## Review and remaining scope

An independent read-only review identified and prompted fixes for client-side
source-policy bypass, journal synchronization, pipelined/disconnected writes,
Windows credential-directory ACLs and initialization-failure retries. The
remaining nonce compatibility observation was handled by bumping the local
service protocol to v2.

Not yet validated: physical NI/82357B bus coexistence, Windows runtime/ACLs,
hardware Stop latency under another slow instrument, and coordinated installer
or portable packaging. Official VISA service support, automatic service startup,
PLE/Camera device adapters and same-device readback subscriptions are not
implemented. No new release was published and no installed desktop app was
replaced during this work.

See [development setup](../guides/shared-instruments-development.md) and the
[cross-application design](../development/shared-instrument-service.md).
