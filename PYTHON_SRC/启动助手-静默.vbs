' 完全无窗口启动抖音弹幕助手（双击此文件，不会闪出任何命令行窗口）
Set fso = CreateObject("Scripting.FileSystemObject")
Set ws = CreateObject("WScript.Shell")
ws.CurrentDirectory = fso.GetParentFolderName(WScript.ScriptFullName)
ws.Run "pythonw main.py", 0, False
