# Application settings — alpha.17 validation

Version `1.3.0-alpha.17` isolates the IV application default at
`~/.kohdalab/iv/config.json` (`%USERPROFILE%\.kohdalab\iv\config.json` on Windows).
The default measurement directory is `iv/results` beneath the same base.
Saved experiment output directories remain authoritative. Windows Setup stays
at `C:\Program Files\Kohdalab\KohdaLab IV` and the macOS bundle is
`KohdaLab IV.app`, intended for `/Applications`.

## Configuration boundary

Only the application-specific default is discovered. Old shared Python/Rust
profiles, including `config/iv-next.json`, are not auto-imported or deleted.
Manual Load/Save and explicit profiles remain available for valid IV settings.
New saves carry `application: "kohdalab-iv"`. Foreign application profiles,
unidentified JSON, damaged settings, and unsupported newer schemas are rejected
before replacement. A fully written, synchronized temporary file is published
atomically. Invalid startup settings produce an error and do not silently fall
back to another application's or old default.

## Verification scope

Temporary-directory regression tests cover the new path, ignored legacy files,
identity/schema checks, byte preservation on rejection, and repeated saves.
Windows installation lifecycle tests use an isolated TestRoot, verify the
app-specific working directory, and preserve saved settings and measurements
across updates. GUI API, simulation, mock safety, packaging policy, documentation,
and platform build checks are retained. The source kit and packaged source carry
the same settings and installer implementation as the compiled application.

This is a storage/packaging change. It does not change instrument commands,
measurement/CSV semantics, protection thresholds, or source output ownership.
No new physical instrument measurements or driver installation qualification
are claimed for this preview. Windows artifacts remain unsigned and macOS uses
ad hoc signing; this release adds no publisher signing or Apple notarization.
