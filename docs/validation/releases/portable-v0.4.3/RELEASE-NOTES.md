KohdaLab IV v0.4.3

Run now uses Start / Stop / Save. Save exports the entire acquired CSV and its
.run.json to a new filename after measurement cleanup. Streaming autosave stays
active, existing files are never overwritten, and Output Off remains available
in Source during acquisition.

Repeat now averages N meter readings at each fixed source target instead of
repeating the whole sweep. Individual conversions are checked against software
limits; early-stop metadata preserves the triggering value and acquired count.

Source range, effective step and ramp are automatic. Tiny unit-conversion noise
no longer blocks valid targets. Start explains invalid settings/connections in
English without raising protection limits. Requested and applied settings are
both retained. Unnecessary intermediate ramps are avoided while explicit
settling times and NPLC remain under user control.

Download the Mac Apple Silicon portable, Windows x64 portable, or source build
kit. Python/Rust/VISA installation is not required for the application. Windows
uses the included WinUSB setup. Mac is ad hoc signed, not Apple-notarized.
82357B support still requires separately authorized firmware initialization;
no firmware acquisition or upload is included. Complete corresponding native
source, GPL/LGPL terms and build instructions remain included.
