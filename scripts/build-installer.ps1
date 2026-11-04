# Builds the release game and the Windows installer (target/installer/).
# Needs Inno Setup 6 (ISCC.exe) and the GeneralUser GS SoundFont in vendor/soundfont/
# (https://github.com/mrbumpy409/GeneralUser-GS, file GeneralUser-GS.sf2 and
# documentation/LICENSE.txt).
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    cargo build --release -p ddj-game
    if ($LASTEXITCODE) { throw 'cargo build failed' }
    if (-not (Test-Path 'vendor/soundfont/GeneralUser-GS.sf2')) { throw 'vendor/soundfont/GeneralUser-GS.sf2 is missing' }
    $iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $iscc) { throw 'Inno Setup 6 (ISCC.exe) not found' }
    & $iscc installer/ddj.iss
    if ($LASTEXITCODE) { throw 'ISCC failed' }
} finally {
    Pop-Location
}
