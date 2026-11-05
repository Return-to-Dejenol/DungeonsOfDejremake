# Publishes a release that the game's updater finds: builds the installer and the Android APK
# for the version in crates/game/Cargo.toml and uploads them as GitHub release v<version> of
# the public fork.
# Bump that version, then commit and push, first; the release is tagged at HEAD.
param([string]$Notes = '')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$version = (Select-String -Path "$root/crates/game/Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
& "$PSScriptRoot/build-installer.ps1"
& "$PSScriptRoot/build-apk.ps1"
gh release create "v$version" "$root/target/installer/DejenolRestoration-Setup-$version.exe" "$root/target/android/DejenolRestoration-$version.apk" --repo silvariasereneblossom/DungeonsOfDejremake --target (git -C $root rev-parse HEAD) --title "Dejenol Restoration Project $version" --notes $Notes
if ($LASTEXITCODE) { throw 'gh release create failed' }
