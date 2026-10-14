# Native roadmap

Current work: continuous I/V monitor, pinned channel cards and bounded time
history; a Rust-only application and native CSV/schema.

Future work must preserve single-owner instrument access, output shutdown,
per-point CSV flushing and documented resolution semantics. Extend device
profiles only after their official protocol has been reviewed and tested.

Historical migration and performance evidence remains in the docs and Git
history; historical results must not be rewritten to match later software.
