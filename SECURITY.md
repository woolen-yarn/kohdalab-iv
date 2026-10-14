# Security

Report suspected vulnerabilities privately to the repository maintainers through
GitHub security reporting where available. Do not publish credentials or personal
instrument-network details in an issue.

Application crates forbid unsafe Rust. Native C/C++ helpers and OS libraries have
separate memory-safety boundaries; they remain isolated processes. Instrument
access uses exclusive device leases and bounded IPC frames.

CI checks Rust dependencies using cargo-audit against RustSec. Keep Cargo.lock,
compiler versions, native helper hashes and license notices reviewable. Do not
replace upstream firmware or driver binaries with unverified downloads.
