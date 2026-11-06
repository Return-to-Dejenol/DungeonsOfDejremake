# Builds the Android APK, target/android/DejenolRestoration-<version>.apk, without Gradle:
# the game as a native library (crates/android) for arm64 and x86_64, the Java activity
# (android/Main.java), and the SoundFont as an asset. Needs the Android SDK (ANDROID_HOME,
# else %LOCALAPPDATA%\Android\Sdk) with build-tools, a platform and the NDK, a JDK, and
# rustup's aarch64-linux-android and x86_64-linux-android targets.
# It signs with ~/.android/dejenol-release.jks, made on the first run: keep that key, since
# an APK signed with another can't update an installed one.
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot)
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { "$env:LOCALAPPDATA\Android\Sdk" }
$latest = { param($dir) (Get-ChildItem $dir | Sort-Object { [version](($_.Name -replace '^android-' -replace '[^0-9.].*$') + '.0') } | Select-Object -Last 1).FullName }
$bt = & $latest "$sdk\build-tools"
$jar = (& $latest "$sdk\platforms") + '\android.jar'
$llvm = (& $latest "$sdk\ndk") + '\toolchains\llvm\prebuilt\windows-x86_64\bin'
$version = (Select-String -Path crates/game/Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$v = [version]$version
$out = 'target/android'
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory "$out/apk", "$out/classes", "$out/res/mipmap", "$out/tmp" | Out-Null
# Temporary files under target/: a checkout with a Low integrity label runs build scripts
# (and the clang they start) at Low, which can't write to %TEMP%.
$env:TEMP = $env:TMP = (Resolve-Path "$out/tmp").Path

foreach ($t in @(('arm64-v8a', 'aarch64-linux-android'), ('x86_64', 'x86_64-linux-android'))) {
    $abi, $triple = $t
    $var = $triple.ToUpper().Replace('-', '_')
    Set-Item "env:CARGO_TARGET_${var}_LINKER" "$llvm\${triple}26-clang.cmd"
    # ring (TLS for cloud saves) compiles C for the phone too.
    Set-Item "env:CC_$($triple.Replace('-', '_'))" "$llvm\${triple}26-clang.cmd"
    Set-Item "env:AR_$($triple.Replace('-', '_'))" "$llvm\llvm-ar.exe"
    cargo build -p ddj-android --release --target $triple
    if ($LASTEXITCODE) { throw "cargo build for $triple failed" }
    New-Item -ItemType Directory "$out/apk/lib/$abi" | Out-Null
    & "$llvm\llvm-strip.exe" -o "$out/apk/lib/$abi/libdejenol.so" "target/$triple/release/libdejenol.so"
}

javac --release 11 -nowarn -cp $jar -d "$out/classes" android/Main.java
if ($LASTEXITCODE) { throw 'javac failed' }
& "$bt\d8.bat" --release --min-api 26 --lib $jar --output "$out/apk" (Get-ChildItem "$out/classes" -Recurse -Filter *.class).FullName
if ($LASTEXITCODE) { throw 'd8 failed' }

Copy-Item assets/icon.png "$out/res/mipmap/ic_launcher.png"
& "$bt\aapt2.exe" compile --dir "$out/res" -o "$out/res.zip"
& "$bt\aapt2.exe" link -o "$out/base.apk" -I $jar --manifest android/AndroidManifest.xml -A vendor/soundfont "$out/res.zip" `
    --min-sdk-version 26 --target-sdk-version 35 --version-name $version --version-code ($v.Major * 10000 + $v.Minor * 100 + $v.Build)
if ($LASTEXITCODE) { throw 'aapt2 link failed' }

# The code and the libraries go in beside what aapt2 made.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::Open((Resolve-Path "$out/base.apk"), 'Update')
foreach ($f in Get-ChildItem "$out/apk" -Recurse -File) {
    $name = $f.FullName.Substring((Resolve-Path "$out/apk").Path.Length + 1).Replace('\', '/')
    [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $f.FullName, $name) | Out-Null
}
$zip.Dispose()
& "$bt\zipalign.exe" -P 16 -f 4 "$out/base.apk" "$out/aligned.apk"
if ($LASTEXITCODE) { throw 'zipalign failed' }

$ks = "$env:USERPROFILE\.android\dejenol-release.jks"
$pass = "$env:USERPROFILE\.android\dejenol-release.pass"
if (-not (Test-Path $ks)) {
    New-Item -ItemType Directory -Force (Split-Path $ks) | Out-Null
    $b = New-Object byte[] 24
    [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($b)
    [Convert]::ToBase64String($b) | Set-Content -NoNewline -Encoding ascii $pass
    keytool -genkeypair -keystore $ks -storepass:file $pass -alias dejenol -keyalg RSA -keysize 4096 -validity 36500 -dname 'CN=Dejenol Restoration Project'
    if ($LASTEXITCODE) { throw 'keytool failed' }
}
$apk = "$out/DejenolRestoration-$version.apk"
& "$bt\apksigner.bat" sign --ks $ks --ks-pass "file:$pass" --out $apk "$out/aligned.apk"
if ($LASTEXITCODE) { throw 'apksigner failed' }
Write-Host "Built $apk"
