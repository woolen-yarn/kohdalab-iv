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
