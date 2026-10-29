# Common control API validation

Date: 2026-10-29. Target: 1.3.0-alpha.18. Host: macOS ARM64.

- Device-free Rust tests: 280 passed across the selected application/workspace and common contract suites.
- Shared Python SDK: 9 tests passed, including authenticated loopback errors, exact job links, ambiguous timeout boundaries and pending cleanup.
- Actual compiled GUI: common and native clients, three synthetic points, 18 CSV columns, duplicate receipts, exclusive ownership, fresh Output OFF and release/reacquire passed.
- Physical instruments were not opened or qualified for this change. Windows packaging/CI evidence belongs to the matching PR/release; these local tests are not a Windows hardware pass.

The existing native API, scientific storage and product-specific safety policy remain separate from the additive common namespace. See [the common contract](../reference/common-control-api.md).
