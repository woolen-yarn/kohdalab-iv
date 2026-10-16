; KohdaLab IV Windows installer shell.
; SPDX-License-Identifier: MIT
;
; This source is MIT-licensed project code. NSIS is distributed under the zlib
; license. Its generated bootstrap code is part of Setup.exe; the NSIS compiler
; is not shipped. The application payload retains support/licenses and SOURCE.zip.
;
; Required compile-time definitions (provided by scripts/build_windows_installer.py):
;   PAYLOAD_DIR  complete, prepared Windows portable directory
;   WRAPPER_PATH Install-Installer.ps1 outside the portable payload
;   OUTPUT_FILE  destination Setup.exe path
;   VERSION      human-readable application version
;   VERSION_FILE four-part numeric Windows file version

Unicode true
SetCompressor /SOLID zlib
SetDatablockOptimize on

!ifndef PAYLOAD_DIR
  !error "PAYLOAD_DIR must name a prepared portable directory"
!endif
!ifndef WRAPPER_PATH
  !error "WRAPPER_PATH must name Install-Installer.ps1"
!endif
!ifndef OUTPUT_FILE
  !error "OUTPUT_FILE must name the Setup.exe output"
!endif
!ifndef VERSION
  !error "VERSION must be set"
!endif
!ifndef VERSION_FILE
  !error "VERSION_FILE must be set to a four-part numeric Windows version"
!endif

!include "MUI2.nsh"
!include "x64.nsh"
!include "LogicLib.nsh"
!define PROJECT_ROOT "${__FILEDIR__}\\..\\.."

Name "KohdaLab IV ${VERSION}"
OutFile "${OUTPUT_FILE}"
InstallDir "$PROGRAMFILES64\Kohdalab\KohdaLab IV"
InstallDirRegKey HKLM "Software\Kohdalab\KohdaLab IV" "InstallLocation"
RequestExecutionLevel admin
ShowInstDetails show
ShowUninstDetails show
BrandingText "KohdaLab IV"
Icon "${PROJECT_ROOT}\\next\\assets\\kohdalab-iv.ico"
UninstallIcon "${PROJECT_ROOT}\\next\\assets\\kohdalab-iv.ico"

VIAddVersionKey /LANG=1033 "ProductName" "KohdaLab IV"
VIProductVersion ${VERSION_FILE}
VIAddVersionKey /LANG=1033 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "FileDescription" "KohdaLab IV Setup"
VIAddVersionKey /LANG=1033 "LegalCopyright" "KohdaLab"

!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "KohdaLab IV Setup"
!define MUI_WELCOMEPAGE_TEXT "This installs KohdaLab IV for all users of this PC.$\r$\n$\r$\nExisting settings and measurement results are kept. USB driver configuration is never changed by this installer."
!define MUI_FINISHPAGE_TITLE "KohdaLab IV is ready"
!define MUI_FINISHPAGE_TEXT "KohdaLab IV was installed successfully. Start it from the Start menu or desktop shortcut when you are ready."

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP|MB_OK "KohdaLab IV requires 64-bit Windows."
    Abort
  ${EndIf}
  SetRegView 64
  ; Ignore a caller-supplied /D path. The PowerShell transaction uses this
  ; fixed Program Files location and validates it before every update.
  StrCpy $INSTDIR "$PROGRAMFILES64\Kohdalab\KohdaLab IV"
FunctionEnd

Function .onInstFailed
  IfSilent +3
    MessageBox MB_ICONSTOP|MB_OK "KohdaLab IV was not installed. No driver settings were changed. Review the installer details, close any running KohdaLab IV process, and try again."
  SetErrorLevel 1
FunctionEnd

Function un.onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP|MB_OK "KohdaLab IV requires 64-bit Windows."
    Abort
  ${EndIf}
  SetRegView 64
  ; Do not honor a caller-supplied /D path for removal. The helper validates
  ; this same fixed Program Files directory before changing any files.
  StrCpy $INSTDIR "$PROGRAMFILES64\Kohdalab\KohdaLab IV"
FunctionEnd

Function un.onUninstFailed
  IfSilent +3
    MessageBox MB_ICONSTOP|MB_OK "KohdaLab IV was not completely removed. The installed files were left in place for safety. Review the uninstaller details and try again."
  SetErrorLevel 1
FunctionEnd

Section "KohdaLab IV" SecMain
  ; Do not present a directory page: the PowerShell wrapper owns the fixed,
  ; validated Program Files update transaction and preserves user data.
  InitPluginsDir
  SetOutPath "$PLUGINSDIR\payload"
  File /r "${PAYLOAD_DIR}\*.*"
  ; The wrapper must be beside payload, never inside it: FILES.json protects
  ; the exact portable tree passed to the installation transaction.
  SetOutPath "$PLUGINSDIR"
  File /oname=Install-Installer.ps1 "${WRAPPER_PATH}"
  WriteUninstaller "$PLUGINSDIR\Uninstall.exe"

  ; Pass paths through the child environment instead of interpolating them into
  ; PowerShell text. This keeps spaces and apostrophes in paths harmless.
  System::Call 'Kernel32::SetEnvironmentVariable(t "KOHDA_SETUP_PACKAGE", t "$PLUGINSDIR\payload") i.r0'
  System::Call 'Kernel32::SetEnvironmentVariable(t "KOHDA_SETUP_WRAPPER", t "$PLUGINSDIR\Install-Installer.ps1") i.r0'
  System::Call 'Kernel32::SetEnvironmentVariable(t "KOHDA_SETUP_UNINSTALLER", t "$PLUGINSDIR\Uninstall.exe") i.r0'
  System::Call 'Kernel32::SetEnvironmentVariable(t "KOHDA_SETUP_VERSION", t "${VERSION}") i.r0'
  DetailPrint "Installing KohdaLab IV..."
  SetOutPath "$TEMP"
  ${DisableX64FSRedirection}
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -Command "$$s=[IO.File]::ReadAllText($$env:KOHDA_SETUP_WRAPPER); & ([ScriptBlock]::Create($$s)) -PackageDirectory $$env:KOHDA_SETUP_PACKAGE -UninstallerSource $$env:KOHDA_SETUP_UNINSTALLER -ApplicationVersion $$env:KOHDA_SETUP_VERSION"'
  Pop $0
  ${EnableX64FSRedirection}
  ${If} $0 == "error"
    DetailPrint "Could not start the installation helper."
    SetErrorLevel 1
    Abort
  ${EndIf}
  ${If} $0 == "timeout"
    DetailPrint "The installation helper timed out."
    SetErrorLevel 1
    Abort
  ${EndIf}
  ${If} $0 != 0
    DetailPrint "Installation wrapper failed with exit code $0."
    SetErrorLevel 1
    Abort
  ${EndIf}
SectionEnd

Section "Uninstall"
  ; NSIS runs its uninstaller from a temporary copy.  The installed script can
  ; therefore delete the original Uninstall.exe only after this process exits.
  IfFileExists "$INSTDIR\Uninstall-Application.ps1" +2 0
    Abort "Uninstall-Application.ps1 is missing; refusing to remove the installation."
  System::Call 'Kernel32::SetEnvironmentVariable(t "KOHDA_UNINSTALL_DIRECTORY", t "$INSTDIR") i.r0'
  System::Call 'Kernel32::SetEnvironmentVariable(t "KOHDA_UNINSTALL_SCRIPT", t "Uninstall-Application.ps1") i.r0'
  DetailPrint "Removing KohdaLab IV..."
  SetOutPath "$TEMP"
  ${DisableX64FSRedirection}
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -Command "$$s=[IO.File]::ReadAllText([IO.Path]::Combine($$env:KOHDA_UNINSTALL_DIRECTORY,$$env:KOHDA_UNINSTALL_SCRIPT)); & ([ScriptBlock]::Create($$s)) -InstallationDirectory $$env:KOHDA_UNINSTALL_DIRECTORY"'
  Pop $0
  ${EnableX64FSRedirection}
  ${If} $0 == "error"
    DetailPrint "Could not start the uninstall helper."
    SetErrorLevel 1
    Abort
  ${EndIf}
  ${If} $0 == "timeout"
    DetailPrint "The uninstall helper timed out."
    SetErrorLevel 1
    Abort
  ${EndIf}
  ${If} $0 != 0
    DetailPrint "Uninstall helper failed with exit code $0."
    SetErrorLevel 1
    Abort
  ${EndIf}
SectionEnd
