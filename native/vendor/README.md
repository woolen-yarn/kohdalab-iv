# Corresponding native dependency source

`ni-gpib` contains KAME's userspace Linux-GPIB driver from commit
`b413cc7102e984fa8e5e13bd4993549a6e4584cc`:
https://github.com/northriv/KAME/tree/b413cc7102e984fa8e5e13bd4993549a6e4584cc/modules/charinterface/usermode-linux-gpib

Local modifications add `setTimeout`, addressed Go-To-Local, and listener
discovery to NiGpibDriver; see `native/gpib_listener.inc`. Windows compatibility
fixes, if needed, are retained directly in the supplied source. The driver and
its Linux-GPIB implementation are GPL-2.0-or-later, with notices in each file
and `ni-gpib/linux-gpib/COPYING`.

`libusb-1.0.30.tar.bz2` is the unmodified upstream release source:
https://github.com/libusb/libusb/releases/tag/v1.0.30
SHA256: `fea36f34f9156400209595e300840767ab1a385ede1dc7ee893015aea9c6dbaf`.
Its COPYING license is LGPL-2.1-or-later. Complete source is supplied because
the Windows helpers statically link this library. Rebuild with
`scripts/build_native_windows.sh` in MSYS2 UCRT64 using GCC, Make, and Bash.

The 82357B adaptation is in `native/agilent_82357b.cpp/.hpp` (GPL-2.0-only).
It is based on Linux v6.17 `drivers/staging/gpib/agilent_82357a` and
`drivers/staging/gpib/include/tms9914.h`:
https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/tree/drivers/staging/gpib?h=v6.17
Its source, USB protocol and initialization changes are included here.
Firmware is downloaded separately and is not redistributed in this archive.
