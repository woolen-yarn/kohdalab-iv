# KohdaLab IV for Linux: simulation only

This Ubuntu 24.04 x64 build (DEB and portable `.tar.gz`) is for simulation only.
It does **not** acquire, discover or control instruments.

- Use `SIMULATED_SOURCE` and `SIMULATED_METER` in the GUI, or
  `iv-cli --measure --simulate --output run.csv`, to try sweeps, plots, CSV
  export and the Python/GUI control API.
- The binaries are compiled with the `simulation-only` Cargo feature. Instrument
  discovery, connection, the shared instrument hub and every USB/GPIB/VISA helper
  launch are refused inside the transport layer. `KOHDALAB_IV_HELPER_DIR`,
  `KOHDALAB_INSTRUMENT_HUB_CONFIG` or helpers copied next to the application do
  not change this.
- No native instrument helper, driver, firmware or udev rule is included or
  installed.

For real instruments, use the Windows or macOS release. Linux hardware support
is not validated; see `SAFETY.md` and `docs/guides/hardware-validation.md` in
`support/SOURCE.zip` before planning any such work.

## Contents

| Path | Purpose |
| --- | --- |
| `kohdalab-iv` | GUI application and measurement engine (simulation only) |
| `iv-cli` | Command-line entry point that runs `kohdalab-iv` |
| `python-control/` | Python GUI/control clients, same as the other platforms |
| `support/SOURCE.zip` | Complete corresponding source, including this packaging |
| `support/licenses/` | Project, native-source and Rust dependency licenses |
| `support/THIRD_PARTY_NOTICES.md` | Third-party notices |
| `FILES.json` | SHA-256 of every file in this payload |

The DEB installs this payload in `/opt/kohdalab-iv`, adds `kohdalab-iv` and
`iv-cli` to `/usr/bin` and a desktop entry named "KohdaLab IV (Simulation)".
Remove it with `sudo apt remove kohdalab-iv`. The portable archive runs in
place after extraction: `./kohdalab-iv`.

Runtime libraries (Ubuntu 24.04 package names): `libegl1 libgl1 libxkbcommon0
libxkbcommon-x11-0 libwayland-client0 libwayland-cursor0 libwayland-egl1
libx11-6 libx11-xcb1 libxcursor1 libxi6 libxrandr2`. File dialogs use
`xdg-desktop-portal`.
