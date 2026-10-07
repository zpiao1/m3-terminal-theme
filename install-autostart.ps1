# Start m3sync.py at login via a Startup-folder shortcut (pythonw: no console).
# Remove the shortcut to disable. Run:  powershell -File install-autostart.ps1
$pyw = Join-Path (Split-Path (Get-Command python3).Source) 'pythonw.exe'
$dir = $PSScriptRoot
$lnk = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup\M3 Terminal Theme.lnk"
$s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
$s.TargetPath = $pyw
$s.Arguments = "`"$dir\m3sync.py`" --log `"$dir\m3sync.log`""
$s.WorkingDirectory = $dir
$s.WindowStyle = 7
$s.Save()
Start-Process $lnk
"Installed: $lnk"
