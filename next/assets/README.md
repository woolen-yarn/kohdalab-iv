# KohdaLab IV application icon

The blue and orange curves represent the forward and backward I-V sweep on a
dark instrument display. The project MIT license also applies to these assets.

- `kohdalab-iv.svg`: editable vector export.
- `kohdalab-iv.png`: transparent 1024 by 1024 master preview.
- `kohdalab-iv-window.png`: embedded 256 by 256 GUI icon.
- `kohdalab-iv.ico`: Windows 16, 24, 32, 48, 64, 128 and 256 pixel icon images.
- `kohdalab-iv.icns`: macOS high-resolution icon set.

The canonical geometry and colors are in `scripts/build_iv_icons.py`. With
Pillow installed, run that script from the repository root to regenerate all
formats together. Windows resource compilation runs automatically from Cargo;
Mac packaging copies the ICNS into the app Resources and sets CFBundleIconFile.
