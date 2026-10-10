# Install m3sync into ~/.cargo/bin and start it at login via a Startup-folder
# shortcut (windows-subsystem exe: no console). Re-run after changing the code.
# Remove the shortcut to disable. Run:  powershell -File install-autostart.ps1
$dir = $PSScriptRoot
# Windows can't replace a running exe, so stop the old copy first.
Stop-Process -Name m3sync -ErrorAction SilentlyContinue
cargo install --locked --path "$dir\m3sync"
if ($LASTEXITCODE -ne 0) { throw "cargo install failed" }
$exe = "$env:USERPROFILE\.cargo\bin\m3sync.exe"
$lnk = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Startup\M3 Terminal Theme.lnk"
$s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
$s.TargetPath = $exe
$s.Arguments = "--log `"$dir\m3sync.log`""
$s.WorkingDirectory = $dir
$s.Save()
Start-Process $lnk
"Installed: $exe, started via $lnk"
