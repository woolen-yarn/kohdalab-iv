Option Explicit

Dim shell, fso, projectRoot, pythonwPath, uvPath, command, env

Set shell = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")

projectRoot = fso.GetParentFolderName(fso.GetParentFolderName(WScript.ScriptFullName))
pythonwPath = projectRoot & "\.venv\Scripts\pythonw.exe"
uvPath = shell.ExpandEnvironmentStrings("%USERPROFILE%") & "\.local\bin\uv.exe"
shell.CurrentDirectory = projectRoot
Set env = shell.Environment("PROCESS")
env("PYTHONPATH") = projectRoot & "\src"

If fso.FileExists(pythonwPath) Then
  command = """" & pythonwPath & """ -m kohdalab_iv.apps.iv_gui"
ElseIf fso.FileExists(uvPath) Then
  command = "cmd.exe /c start ""KohdaLab IV"" /D """ & projectRoot & """ """ & uvPath & """ run --extra gui pythonw -m kohdalab_iv.apps.iv_gui"
Else
  MsgBox "Neither pythonw.exe nor uv.exe was found.", vbCritical, "KohdaLab IV"
  WScript.Quit 1
End If

' Use window style 0 to hide launcher plumbing. The Qt app calls showNormal
' after startup so the main GUI window is still brought to the foreground.
shell.Run command, 0, False
