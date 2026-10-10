# >>> m3-terminal-theme >>>
# Material 3 Expressive prompt + PSReadLine colours. ~/m3-terminal-theme
# (m3sync) regenerates palette.ps1 from the wallpaper with every escape
# string precomputed ($M3 = prompt pieces, $M3Syntax = PSReadLine colours);
# this block only renders them. It runs on every prompt, so it uses .NET calls
# rather than cmdlets. Must stay ABOVE the intelligent-terminal block so its
# wrapper chains this prompt.
$global:__m3 = @{ Path = Join-Path $HOME '.config\m3-theme\palette.ps1'; Stamp = $null; PSRLStamp = $null }
function global:__m3_refresh {
    $stamp = [IO.File]::GetLastWriteTimeUtc($__m3.Path)    # 1601-01-01 when missing
    if ($stamp.Year -eq 1601) { return }
    if ($stamp -ne $__m3.Stamp) { . $__m3.Path; $__m3.Stamp = $stamp }
    # PSReadLine may load after the palette was first read, so track it apart.
    if ($stamp -ne $__m3.PSRLStamp -and (Get-Module PSReadLine)) {
        Set-PSReadLineOption -Colors $M3Syntax
        $__m3.PSRLStamp = $stamp
    }
}
function global:prompt {
    $ok = $?
    __m3_refresh
    $cwd = $executionContext.SessionState.Path.CurrentLocation.ProviderPath
    if (-not $M3) { return "PS $cwd> " }
    $e = [char]27; $rst = "$e[0m"

    $path = $cwd
    if ($path.StartsWith($HOME, 'OrdinalIgnoreCase')) { $path = '~' + $path.Substring($HOME.Length) }

    # Find .git by walking up (no git.exe spawn), then read HEAD directly.
    $branch = $null; $dir = $cwd
    while ($dir) {
        $head = "$dir\.git\HEAD"
        if ([IO.File]::Exists($head)) {
            # HEAD is read raw (git's own ref-name checks don't apply to a .git
            # dir unpacked from an archive), so drop control chars: an ESC in
            # it would otherwise reach the terminal as an escape sequence.
            $ref = [IO.File]::ReadAllText($head).Trim() -replace '[\x00-\x1f\x7f-\x9f]', ''
            $branch = if ($ref -match '^ref: refs/heads/(.+)$') { $Matches[1] } else { $ref.Substring(0, [Math]::Min(7, $ref.Length)) }
            break
        }
        $dir = [IO.Path]::GetDirectoryName($dir)
    }

    $s = "$rst$($M3.DirEdge)$([char]0xE0B6)$($M3.DirPill)$e[1m $path "
    $edge = $M3.DirEdge
    if ($branch) {
        $s += "$rst$($M3.DirToBranch)$([char]0xE0B0)$($M3.BranchPill)$e[1m $([char]0xE0A0) $branch "
        $edge = $M3.BranchEdge
    }
    $arrow = if ($ok) { $M3.Ok } else { $M3.Err }
    "$s$rst$edge$([char]0xE0B4)$rst $arrow$e[1m$([char]0x276F)$rst "
}
# <<< m3-terminal-theme <<<

