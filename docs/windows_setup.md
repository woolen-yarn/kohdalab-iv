# Windows Instrument PC Setup

This checklist prepares a Windows PC for running KohdaLab IV with real
instruments.

## 1. Install System Tools

Install Git:

```powershell
winget install --id Git.Git -e --source winget
```

Install uv:

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
```

Open a new PowerShell and check:

```powershell
git --version
uv --version
```

## 2. Install Instrument Drivers

Install the drivers needed by the connected instruments:

- NI-VISA or Keysight VISA runtime
- GPIB adapter driver
- USBTMC or vendor USB driver when needed

Confirm that VISA resources are visible with the vendor tool before starting
Python.

## 3. Clone and Sync

```powershell
New-Item -ItemType Directory -Force -Path $HOME\pythonKernel
Set-Location $HOME\pythonKernel
git clone https://github.com/Kohdalab/kohdalab-iv.git kohdalab-iv
Set-Location kohdalab-iv
uv sync --all-extras
```

## 4. Create a Local Config

```powershell
Copy-Item config\default.json config\instrument.local.json
```

Edit `config\instrument.local.json` for this PC's VISA resource strings.
Local configs are ignored by Git.

## 5. Check Resources

```powershell
uv run kohdalab-iv list-resources
uv run kohdalab-iv --config config\instrument.local.json check-config
```

## 6. First Hardware Smoke Test

Use the smallest safe source range.

1. Confirm wiring and polarity.
2. Confirm source mode and compliance.
3. Run `check-config`.
4. Start with a short measurement.
5. Confirm the source output turns off at the end.
6. Confirm CSV output in `results`.

## 7. Start GUI or Notebook

GUI:

```powershell
uv run --extra gui kohdalab-iv-gui
```

Notebook:

```powershell
uv run --extra notebook jupyter lab
```
