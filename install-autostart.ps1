# Start m3sync at login via a Startup-folder shortcut (windows-subsystem exe:
# no console). Build first: cargo build --release  (in m3sync/)
# Remove the shortcut to disable. Run:  powershell -File install-autostart.ps1
$dir = $PSScriptRoot
$exe = "$dir\m3sync\target\release\m3sync.exe"
if (-not (Test-Path $exe)) { throw "Build first: cargo build --release in $dir\m3sync" }
$lnk = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup\M3 Terminal Theme.lnk"
$s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
$s.TargetPath = $exe
$s.Arguments = "--log `"$dir\m3sync.log`""
$s.WorkingDirectory = $dir
$s.Save()
Start-Process $lnk
"Installed: $lnk"
