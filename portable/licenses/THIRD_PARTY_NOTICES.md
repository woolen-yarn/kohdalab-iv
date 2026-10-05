# Third-party notices

The GUI/application and USBTMC helper are MIT licensed. The separate combined
GPIB helper contains GPL-2.0 code from KAME and Linux-GPIB; corresponding
source and rebuild instructions are in support/SOURCE.zip (native/vendor,
native and scripts). Windows USB setup also includes libwdi (LGPL-3.0);
its source and rebuilding instructions are supplied in the same archive. libusb 1.0.30 is
LGPL-2.1-or-later; its source archive and license are included for both helpers.

Qt/PySide6 6.11.1 and shiboken6 use LGPL-3.0/GPL alternatives; the GUI uses
the shared framework distribution. License texts are included here. Upstream
source: https://code.qt.io/cgit/pyside/pyside-setup.git/tag/?h=v6.11.1 and
https://code.qt.io/cgit/qt/qtbase.git/tag/?h=v6.11.1 . Rebuild instructions
allow replacement of runtime components in a rebuilt app.

pyqtgraph 0.14.0 and PyVISA 1.16.2 are MIT licensed. NumPy 2.4.6 uses BSD
and bundled third-party licenses. CPython retains its PSF license. The app
bundle retains dependency license files where supplied by their distributions.
PyInstaller's bootloader uses GPL with its distribution exception.
