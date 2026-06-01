Option Explicit

Dim shell
Dim fso
Dim projectRoot
Dim pythonwPath
Dim uvPath
Dim command

Set shell = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")
projectRoot = "C:\Users\kohdalab\pythonKernel\kohdalab-iv"
pythonwPath = projectRoot & "\.venv\Scripts\pythonw.exe"
uvPath = shell.ExpandEnvironmentStrings("%USERPROFILE%") & "\.local\bin\uv.exe"

If fso.FileExists(pythonwPath) Then
  command = "cmd.exe /c cd /d """ & projectRoot & """ && set PYTHONPATH=src && """ & pythonwPath & """ -m kohdalab_iv.apps.iv_gui > gui_launcher.log 2>&1"
Else
  command = "cmd.exe /c cd /d """ & projectRoot & """ && """ & uvPath & """ run --extra gui python -m kohdalab_iv.apps.iv_gui > gui_launcher.log 2>&1"
End If

shell.Run command, 0, False
