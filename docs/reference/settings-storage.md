# Application-specific settings

Each application owns its own directory beneath the user's `.kohdalab` folder.
On Windows the base is `%USERPROFILE%\.kohdalab`; on macOS/Linux it is
`~/.kohdalab`.

| Application | Default configuration | Identity |
| --- | --- | --- |
| KohdaLab IV | `iv/config.json` | `kohdalab-iv` |
| KohdaLab TRKR | `trkr/config.json` | `kohdalab-trkr` |
| TRKR demo | `trkr/demo.json` | `kohdalab-trkr` |

New saves contain an `application` field. A different explicit application
identity is rejected when loading and before replacing an existing file on Save.
Recognized markerless foreign profile structures are also rejected. Markerless
native legacy profiles remain readable for compatibility.

## IV migration

On a normal first launch, IV prefers `iv/config.json`. If it does not exist,
a valid native Rust `config/iv-next.json` is imported into the new location.
The old file remains byte-for-byte intact. A temporary file is validated and
published without replacing a concurrently created destination. A later launch
always prefers the new file, even when the old configuration still exists.

An invalid or newer-schema configuration is reported rather than silently
falling back to an older configuration or overwriting that file with defaults.
Explicit `--config` launches and GUI simulation checks do not migrate the user's
default configuration. Migration only reads and writes settings; it never
connects a device or enables output.

The old Python IV `config/iv.json` and TRKR's `config/default.json` have different
schemas and are not imported by IV. Keep those originals when using old software.

## TRKR migration

The TRKR implementation imports its native `config/trkr-next.json` first, then
its legacy `config/default.json` when the new default is absent. Demo settings
belong to `trkr/demo.json` and do not replace production settings. Follow the
TRKR repository's configuration documentation for that application's migration
and validation details.

## Manual profiles

Browse/Load/Save can still select an explicit profile path for an experiment.
Use a separate file per application; an identity marker does not convert one
application's profile into another format. Existing experiment data and selected
output directories are independent from the default configuration location.
