# Security Policy

This project is laboratory-control software. A security issue may include any
bug that allows unintended instrument output, unsafe remote operation, leaked
credentials, or corrupted measurement files.

## Reporting

Please report sensitive issues through GitHub private security advisories when
available. If that is not available, contact the repository maintainers through
the KohdaLab organization before publishing details.

Include:

- affected repository and version or commit
- operating system and instrument connection type
- steps to reproduce
- whether any instrument output was enabled
- suggested mitigation if known

## Scope

In scope:

- unsafe source output behavior
- missing output-off behavior on errors
- command injection or unsafe parsing
- leaked local configuration or credentials
- vulnerabilities in the PC-side control tools

Out of scope:

- unsupported hardware modifications
- failures caused by bypassing documented safety checks
- third-party instrument firmware vulnerabilities

## Dependency Auditing

The lock file is checked against the Python Packaging Advisory Database locally
and in the weekly Security workflow:

```bash
uv run --group audit python scripts/check_project.py audit
```

The command exports a temporary standardized lock from `uv.lock`, audits all
runtime, optional, development, packaging, and audit dependencies, and removes
the temporary file afterward. Do not suppress an advisory without documenting
why the affected code path is unreachable and when the suppression will expire.
