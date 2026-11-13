@echo off
cd /d "%~dp0"
"%~dp0USB-Setup.exe"
if errorlevel 1 (
  echo USB setup did not complete. See the displayed error and DRIVERS.md.
  pause
)
