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
New-Item -ItemType Directory -Force "$HOME\KohdaLab\config"
Copy-Item src\kohdalab_iv\resources\default.json "$HOME\KohdaLab\config\iv.local.json"
```

Edit `$HOME\KohdaLab\config\iv.local.json` for this PC's VISA resource
strings and safety limits. Keeping it outside the repository prevents accidental
commits and preserves it across repository updates.

## 5. Check Resources

```powershell
uv run kohdalab-iv list-resources
uv run kohdalab-iv --config "$HOME\KohdaLab\config\iv.local.json" check-config
```

## 6. First Hardware Smoke Test

Follow the complete [hardware smoke-test guide](hardware_smoke_test.md). Use the
smallest safe source range and confirm zero/output-off behavior before expanding
the sweep.

## 7. Start GUI or Notebook

GUI:

```powershell
$env:KOHDALAB_IV_CONFIG = "$HOME\KohdaLab\config\iv.local.json"
uv run --extra gui kohdalab-iv-gui
```

Notebook:

```powershell
uv run --extra notebook jupyter lab
```
