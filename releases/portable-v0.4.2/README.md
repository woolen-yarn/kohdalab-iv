# Portable v0.4.2

Mac ARM64 and Windows x64 native distributions and the source build kit.

This release removes unverified 82357B firmware download and RAM upload. Only
an already initialized 0957:0718 adapter is supported; boot0957:0518 is refused
without replacing its boot driver or opening/uploading it. NI HS+ and direct
USBTMC remain supported. No external firmware is bundled or obtained.

Keep REDISTRIBUTION.md, support/SOURCE.zip, licenses and notices with redistributed
binaries. The GPLv2 combined GPIB executable is separate from the MIT Rust
application; the standalone protocol and licensing evidence are supplied.
Full source and LGPL library rebuilding/relinking instructions are included.

Validation records are in the release notes. Historical v0.4.1 and earlier
packages are unchanged; the revised redistribution policy applies to v0.4.2.
