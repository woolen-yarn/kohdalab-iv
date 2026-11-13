# Application directories

Each application owns its installation and its user configuration. Windows
installers require administrator approval and use separate directories beneath
`C:\Program Files\Kohdalab`. macOS applications belong in `/Applications`.
Portable packages remain independent of installation.

| Application | Windows installation directory | macOS application | User configuration |
| --- | --- | --- | --- |
| Camera | `KohdaLab Camera` | `KohdaLab Camera.app` | `camera/config/config.json` |
| IV | `KohdaLab IV` | `KohdaLab IV.app` | `iv/config/config.json` |
| TRKR | `KohdaLab TRKR` | `KohdaLab TRKR.app` | `trkr/config/config.json` |
| PLE | `KohdaLab PLE` | `KohdaLab PLE.app` | `ple/config/config.json` |
| Shutter | `KohdaLab Shutter` | `KohdaLab Shutter.app` | `shutter/config/config.json` (desktop preferences only) |

Configuration paths are relative to `~/.kohdalab` on macOS/Linux and
`%USERPROFILE%\.kohdalab` on Windows. TRKR Demo uses `trkr/config/demo.json`.
Runtime API descriptors use the sibling `<app>/run` directory. Results and application logs use `<app>/results` and `<app>/logs` where applicable. Shared hardware coordination and OS driver caches retain their dedicated namespaces; they are not application configuration files.

New saves identify their owner with `application: "kohdalab-<app>"`.
Foreign, damaged, unidentified, or newer unsupported IV settings are rejected
before replacement. Saves use a temporary file and atomic replacement.

On normal first launch, IV creates a missing `iv/config/config.json` from built-in defaults using a complete temporary file and atomic no-clobber publication. Existing bytes remain unchanged, including invalid or unsupported files; load/create errors are shown in the GUI. Concurrent initialization reads the winning file rather than replacing it. Explicit profiles and smoke-test launches do not initialize the production default. Creating a profile does not connect instruments or enable output.

IV and TRKR read only their dedicated defaults. Files such as
`config/iv-next.json`, `config/trkr-next.json`, `config/default.json`, and the old
Python `config/iv.json` are no longer discovered or migrated automatically.
Existing originals are not deleted. Browse/Load/Save and explicit `--config`
profiles can still select a valid application-specific experiment profile.
No device connection or output enable is performed by loading settings.

IV's new default results directory is `iv/results` beneath the same base.
An explicitly saved experiment output directory remains unchanged. Updates and
uninstall preserve user settings and measurement data.

Shutter keeps calibration/network settings and shared device/channel names on the Pico; its local file stores desktop preferences only.
