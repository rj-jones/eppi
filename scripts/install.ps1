# Builds Eppi and installs it for this Windows user: copies it to
# %LOCALAPPDATA%\Programs\Eppi and adds an "Eppi" entry to the Start menu,
# which Windows Search finds. Run it again after changing the code.
# -Uninstall removes both (your index and settings stay; delete
# %LOCALAPPDATA%\Eppi and %APPDATA%\Eppi too to remove everything).
#
#   pwsh -File scripts\install.ps1
#   pwsh -File scripts\install.ps1 -Uninstall
param([switch]$Uninstall)
$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $env:LOCALAPPDATA 'Programs\Eppi'
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Eppi.lnk'

if (Get-Process eppi -ErrorAction SilentlyContinue) {
    throw 'Close Eppi first, then run this again.'
}

if ($Uninstall) {
    Remove-Item $shortcut -ErrorAction SilentlyContinue
    Remove-Item $dest -Recurse -ErrorAction SilentlyContinue
    'Removed Eppi from the Start menu and from ' + $dest
    return
}

Push-Location $repo
try {
    cargo build --release -p eppi
    if ($LASTEXITCODE -ne 0) { throw 'cargo build --release failed.' }
} finally {
    Pop-Location
}

New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item (Join-Path $repo 'target\release\eppi.exe') $dest -Force

$link = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
$link.TargetPath = Join-Path $dest 'eppi.exe'
$link.WorkingDirectory = $dest
$link.Description = 'Your Slippi replays: sets, stats, ranks and opponents'
$link.Save()

"Installed to $dest"
"Start menu entry: Eppi"
