Option Explicit

Dim shell, fso, projectRoot, pythonwPath, uvPath, command

Set shell = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")

projectRoot = fso.GetParentFolderName(fso.GetParentFolderName(WScript.ScriptFullName))
pythonwPath = projectRoot & "\.venv\Scripts\pythonw.exe"
uvPath = shell.ExpandEnvironmentStrings("%USERPROFILE%") & "\.local\bin\uv.exe"

If fso.FileExists(pythonwPath) Then
  command = "cmd.exe /c cd /d """ & projectRoot & """ && set PYTHONPATH=src && """ & pythonwPath & """ -m kohdalab_iv.apps.iv_gui > gui_launcher.log 2>&1"
ElseIf fso.FileExists(uvPath) Then
  command = "cmd.exe /c cd /d """ & projectRoot & """ && """ & uvPath & """ run --extra gui python -m kohdalab_iv.apps.iv_gui > gui_launcher.log 2>&1"
Else
  MsgBox "Neither pythonw.exe nor uv.exe was found.", vbCritical, "KohdaLab IV"
  WScript.Quit 1
End If

shell.Run command, 0, False
