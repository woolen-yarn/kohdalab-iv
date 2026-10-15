@echo off
setlocal EnableExtensions
set "KOHDA_PACKAGE_DIR=%~dp0"

rem Load the installer as text.  This works even when PowerShell script execution
rem is restricted; no execution-policy setting is changed on this computer.
powershell.exe -NoProfile -Command "$p=[IO.Path]::GetFullPath($env:KOHDA_PACKAGE_DIR); $s=Join-Path $p 'Install-Application.ps1'; if(-not (Test-Path -LiteralPath $s -PathType Leaf)){throw 'Install-Application.ps1 is missing from this portable package.'}; & ([ScriptBlock]::Create([IO.File]::ReadAllText($s))) -PackageDirectory $p"
set "KOHDA_EXIT=%ERRORLEVEL%"
if not "%KOHDA_EXIT%"=="0" (
  echo.
  echo KohdaLab IV was not installed. Review the error above.
  pause
)
exit /b %KOHDA_EXIT%
