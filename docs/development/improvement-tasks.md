# Improvement tasks for Codex / 改善タスク

Six independent tasks derived from a review of the workspace (6 crates, ~46k LOC),
CI, CHANGELOG and CONTRIBUTING. Each task is self-contained: do them in any order,
one PR per task. Read `CONTRIBUTING.md` before starting; the guardrails below are
non-negotiable.

ワークスペース（6クレート・約46k行）、CI、CHANGELOG、CONTRIBUTING をレビューして
導出した、独立な6タスクです。各タスクは単独で完結し、順不同・1タスク1PRで進められます。
着手前に `CONTRIBUTING.md` を読むこと。下記のガードレールは厳守。

## Non-negotiable guardrails / 厳守事項

Apply to every task below. From `CONTRIBUTING.md`, `ROADMAP.md`, `SAFETY.md`:

- Never relax scientific limits, golden data, or output-shutdown requirements to
  make tests pass.
- Preserve the serialized CSV schema, run-metadata version, settings schema, and
  wire protocol labels. Bump a version only when its compatibility contract
  actually changes (none of these tasks should need a bump).
- Keep GUI rendering free of instrument I/O; keep display history bounded.
- Do not rewrite historical validation results or past CHANGELOG claims to match
  newer software (ROADMAP).
- A successful build alone does not prove unchanged behavior. For any refactor,
  move existing tests with their assertions and compare GUI simulation + saved
  records before/after.
- Required local checks for every PR:
  ```sh
  cargo fmt --all --check
  cargo clippy --workspace --all-targets --locked -- -D warnings
  cargo test --workspace --locked
  ```

## Branch / PR policy

- One branch + one PR per task. Branch from the default branch.
- PR base is the default branch unless `CONTRIBUTING.md` says otherwise.
- Follow the repo's existing commit-message style (imperative summary, as seen in
  `git log`).

---

## Task 1 — Clarify the two version lineages in CHANGELOG / 二系譜の明示

Priority: High. Risk: Low (docs only).

### Problem
`Cargo.toml` is `version = "1.0.7"`. `CHANGELOG.md` lists `1.0.7 … 1.0.1`, then
jumps to `1.4.8 / 1.4.7 … 1.4.0 … 1.3.0-alpha.*`. These are two different release
lineages (the `1.4.x`/`1.3.x` entries use separate tags such as
`portable-v1.4.2`). A reader cannot tell which line is current or why `1.0.1` is
followed by `1.4.8`.

### Scope
- Edit `CHANGELOG.md` only. Do NOT renumber, merge, reorder, or reword any
  existing historical entry (ROADMAP forbids rewriting history).
- Insert clear section separators so the file reads as two explicit lineages,
  e.g. a top heading for the current `1.0.x` stable line and a second heading
  (bilingual EN/JA, matching the file's existing style) introducing the earlier
  `1.4.x`/`1.3.x` lineage, with a one-sentence note on how they relate (separate
  release tags; current stable is the `1.0.x` line).
- Keep the existing bilingual EN-then-JA convention and the language-navigation
  note already present in the file.

### Acceptance criteria
- No existing version number, date, or claim is altered.
- A first-time reader can identify the current stable line and understand the
  jump from `1.0.1` to `1.4.8` from the headings/note alone.
- `git diff` shows only added headings/notes, no edits to historical bullet text.

---

## Task 2 — Align the pinned toolchain with the MSRV / ツールチェーン整合

Priority: High. Risk: Low.

### Problem
`rust-toolchain.toml` pins `channel = "1.99.0"` while the workspace MSRV is
`rust-version = "1.92"` (`Cargo.toml`), and `kohdalab-control-protocol` already
inherits `rust-version.workspace = true`. CHANGELOG 1.4.5 documents the deliberate
move to "Rust 1.92 minimum". Pinning a much newer channel than the declared MSRV
is inconsistent and risks CI toolchain-install flakiness.

### Scope
- Decide one of two directions and apply it consistently:
  - (Preferred) Set `rust-toolchain.toml` `channel` to `"1.92.0"` to match the
    MSRV, and update the CI step in `.github/workflows/test.yml`
    (`rustup toolchain install 1.99.0 …`) to the same version. Grep for every
    other hardcoded `1.99.0` and align it.
  - (Alternative) If a newer channel is intentional, keep it but add a short
    comment in `rust-toolchain.toml` explaining why, and confirm MSRV in
    `Cargo.toml` is still the contract CI enforces.
- Search the repo for `1.99.0` and `1.92` across `*.toml`, `*.yml`, `*.md`,
  `*.ps1`, scripts, and docs; reconcile any now-stale references.

### Acceptance criteria
- `rust-toolchain.toml`, `Cargo.toml` MSRV, and every CI/workflow toolchain-install
  step reference a single, consistent Rust version (or the exception is documented
  with a comment).
- `cargo fmt/clippy/test` pass on the chosen toolchain.
- No measurement, settings, CSV, or safety behavior changes.

---

## Task 3 — Split the oversized top-level modules / 巨大モジュール分割

Priority: Medium. Risk: Medium (behavior-preserving refactor).

### Problem
Top-level files are very large: `iv-desktop/src/ui.rs` (~5,150 lines),
`iv-desktop/src/engine.rs` (~4,640), `iv-core/src/lib.rs` (~3,170),
`iv-transport/src/lib.rs` (~3,020). The `engine/` and `ui/` subdirectories already
show the intended decomposition pattern; the top-level files simply never got the
same treatment.

### Scope
- Pure, behavior-preserving refactor. No logic, numeric, or contract changes.
- Pick ONE file per PR (recommend starting with `iv-desktop/src/ui.rs`) to keep
  diffs reviewable. Do not do all four in one PR.
- Extract cohesive groups into sibling modules, following CONTRIBUTING's existing
  responsibility map. Suggested cuts (adjust to the real seams you find):
  - `ui.rs`: app-state struct & update loop / event sync / layout wiring /
    file-task plumbing — each into a submodule under `ui/` or an `ui/app/` folder.
  - `iv-core/src/lib.rs`: `Plan`/`SweepPoint`/`BalancedPlan`, `Record`/`CsvWriter`,
    and config types into `plan.rs` / `record.rs` / `config.rs`.
  - `iv-transport/src/lib.rs`: group by driver vs. lease vs. routing.
- Keep UI helper modules private to the UI (CONTRIBUTING). Keep CSV serialization
  in `iv-core`.
- Move existing tests to live beside the code they cover, preserving every
  assertion. `crates/iv-desktop/tests/live_engine.rs` must still exercise the
  process interface and output cleanup unchanged.

### Acceptance criteria
- Public API of each crate is unchanged (no new `pub` surface unless strictly
  needed for the module split, and then minimal/`pub(crate)`).
- `cargo test --workspace --locked` passes with the same set of tests (moved, not
  deleted or weakened).
- GUI simulation output and a saved measurement bundle are byte-identical before
  and after (spot-check the analysis CSV + raw CSV + run metadata). Note the
  comparison method in the PR description.
- Each touched file is meaningfully smaller; no file left over ~1,500 lines where
  a clean seam existed.

---

## Task 4 — Add code-coverage reporting to CI / カバレッジ計測の追加

Priority: Medium. Risk: Low.

### Problem
CI runs fmt, clippy `-D warnings`, the full test suite, and packaging-policy
tests, but has no coverage measurement. The safety-critical paths (`engine/`,
`engine/safety.rs`, `engine/stop.rs`, `engine/gate.rs`, `engine/workflow.rs`)
deserve a visible, tracked coverage number to prevent silent regressions.

### Scope
- Add a `cargo llvm-cov` job (preferred over tarpaulin for accuracy) to
  `.github/workflows/test.yml`, or a dedicated `coverage.yml`. Run on Linux only
  to keep it cheap; it must NOT connect hardware, enable outputs, or alter drivers
  (same rules as existing tests).
- Generate an lcov/HTML report as a build artifact. Optionally wire a coverage
  badge, but only if a report host is already acceptable to the maintainers — do
  not add a third-party upload token without asking; prefer artifact-only first.
- Keep the job non-blocking initially (informational), or set a conservative
  threshold agreed with maintainers. Do not fail existing PRs on day one.
- Document how to run coverage locally in `docs/development/` (one short section).

### Acceptance criteria
- A coverage report is produced as a CI artifact on push/PR.
- The job reuses the existing device-free test invocation; no new hardware or
  output activation.
- Coverage numbers for the `engine/` safety modules are visible in the report.
- No secrets/tokens added without explicit maintainer approval (artifact-only is
  fine).

---

## Task 5 — Remove production-code `.unwrap()` on I/O paths / 本番unwrapの排除

Priority: Medium. Risk: Medium.

### Problem
Most `.unwrap()` calls are inside `#[cfg(test)]` modules and are fine. A small
number live in non-test production code, concentrated in
`crates/iv-transport/src/legacy_models.rs` and `crates/iv-desktop/build.rs`, with
a few in UI/client glue. In instrument-I/O paths, a malformed device response
reaching an `unwrap()` becomes a process panic — CONTRIBUTING explicitly requires
drivers to handle "timeout, disconnect and malformed responses".

### Scope
- First, enumerate real production unwraps precisely (exclude `#[cfg(test)]`
  modules and `tests/` dirs). Use clippy rather than grep heuristics:
  temporarily enable `#![warn(clippy::unwrap_used, clippy::expect_used)]` (or a
  crate-level attribute) to get an authoritative list.
- Triage each hit:
  - On any instrument-communication / response-parsing path: replace with proper
    error propagation (`Result` + the crate's classified error type). Do NOT let
    a legacy helper string control shutdown (see `state.rs` / `error_kind`
    contract in CONTRIBUTING).
  - On genuinely-infallible static setup (e.g. a compile-time-known constant,
    `build.rs` host-tool code): either justify with a comment + `expect("reason")`
    that documents the invariant, or restructure to remove it.
- Prefer small, reviewable PRs; `legacy_models.rs` and `build.rs` can be separate
  PRs if the diffs are large.
- Consider leaving a lint guard in place for production code
  (`clippy::unwrap_used` at `warn` or `deny` scoped to non-test code) so the class
  of bug cannot silently return — but only if it passes cleanly after the fixes.

### Acceptance criteria
- No `.unwrap()`/`.expect()` on any instrument-I/O or device-response-parsing path
  in production code; remaining ones are either removed or converted to
  documented-invariant `expect("why")` off the I/O path.
- Malformed/short/disconnected device responses return a classified error instead
  of panicking (add/extend a test that feeds a malformed response and asserts a
  clean error, not a panic — mock transport only, no hardware).
- Output-shutdown precedence and `error_kind` classification unchanged.
- Full checks pass.

---

## Task 6 — Scope out Linux beyond simulation-only / Linux対応の検討

Priority: Low. Risk: Low (investigation + docs; no behavior change yet).
Status: design note in [`linux-visa.md`](linux-visa.md) (#27); ROADMAP item added.

### Problem
The download matrix marks Linux "Simulation only". Research users often want
Linux VISA (NI-VISA / pyvisa-style) support. ROADMAP requires extending device
profiles only after the official protocol is reviewed and tested, so this task is
an investigation + roadmap entry, not a driver change.

### Scope
- Produce a short design note under `docs/development/` describing what a
  Linux-with-real-VISA path would require: which transport backend(s) in
  `iv-transport/` are platform-gated today, what the official Linux VISA story is
  for the supported instruments, and what the existing single-owner-lease /
  output-shutdown guarantees imply for a Linux backend.
- Identify the smallest safe first step (e.g. validating the official VISA C
  shim on Linux against the existing mock GPIB tests) that does NOT require new
  physical-device qualification.
- Add a tracked item to `ROADMAP.md` (follow its existing bilingual structure and
  the "review official protocol before extending" rule). Do NOT claim any new
  physical validation.

### Acceptance criteria
- A design note exists in `docs/development/` with concrete, file-level findings
  about what is platform-gated and why.
- `ROADMAP.md` has a new, clearly-scoped item consistent with its existing
  constraints and bilingual format.
- No driver/runtime behavior changes in this PR; no new physical-validation
  claims; existing simulation-only packaging is unchanged.

---

## Suggested order

1. Task 2 (toolchain) and Task 1 (CHANGELOG) — quick, docs/config, high clarity payoff.
2. Task 4 (coverage) — gives a safety net before the refactors.
3. Task 5 (unwrap hardening) — targeted, testable.
4. Task 3 (module split) — largest, do one file per PR with coverage in place.
5. Task 6 (Linux scoping) — investigation, no deadline pressure.
