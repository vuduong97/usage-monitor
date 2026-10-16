# Build the Rust app and dist\Clawd-<version>-windows-setup.exe (run on Windows).
#   cargo build --release -p clawd-app  ->  Clawd.exe  ->  ISCC (packaging-rs\windows\clawd.iss)
# Build-time settings come from the environment (the release workflow sets them from its secrets):
#   CLAWD_FEED_URL, CLAWD_FEED_TOKEN   firmware feed, baked into the exe (never in the repo)
#   CLAWD_MINISIGN_PUBKEY              minisign public key that must verify SHA256SUMS
#   CLAWD_PRIVATE_FIRMWARE=1           also ship clawd_desktop\firmware and \renderer (clock owners
#                                      only: never a public release)
#   CARGO_TARGET_DIR                   where cargo puts target\ (default desktop-rs\target)
#   PYTHON                             interpreter for check_version.py (default: python)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
$root = (Get-Location).Path

function Invoke-Native([string]$what, [scriptblock]$run) {
    & $run
    if ($LASTEXITCODE -ne 0) { throw "$what failed (exit $LASTEXITCODE)" }
}

$py = if ($env:PYTHON) { $env:PYTHON } else { "python" }
$version = (& $py packaging-rs/check_version.py).Trim()
if ($LASTEXITCODE -ne 0 -or -not $version) { throw "Cargo.toml and tauri.conf.json disagree on the version" }

$cargoArgs = @("build", "--release", "-p", "clawd-app", "--manifest-path", "desktop-rs/Cargo.toml",
               "--features", "tauri/custom-protocol")   # what `tauri build` turns on: pages come from the exe
if (Test-Path desktop-rs\Cargo.lock) { $cargoArgs += "--locked" }
Invoke-Native "cargo build" { cargo @cargoArgs }

$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "desktop-rs\target" }
$exeDir = Join-Path $targetDir "release"
$exe = Join-Path $exeDir "Clawd.exe"
# The installer, the autostart entry and the updater all name Clawd.exe: assert it, exact case.
if (-not (Test-Path -LiteralPath $exe)) { throw "cargo did not produce $exe" }
if ((Get-Item -LiteralPath $exe).Name -cne "Clawd.exe") { throw "binary is not named Clawd.exe: $((Get-Item -LiteralPath $exe).Name)" }
Write-Host ("Clawd.exe: {0:N0} bytes (reported only, not gated)" -f (Get-Item -LiteralPath $exe).Length)

$isccArgs = @("/DAppVersion=$version", "/DExeDir=$exeDir")
if ($env:CLAWD_PRIVATE_FIRMWARE -eq "1") {
    if ($env:GITHUB_ACTIONS -eq "true") { throw "CLAWD_PRIVATE_FIRMWARE=1 is not allowed in CI: public artifacts carry no firmware" }
    $fw = Join-Path $root "clawd_desktop\firmware"
    if (-not (Test-Path -LiteralPath (Join-Path $fw "firmware.json"))) { throw "no firmware bundle in $fw (tools/bundle_firmware.py)" }
    $stage = Join-Path $root "dist\rs-resources"
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory -Path $stage | Out-Null
    Copy-Item -Recurse -LiteralPath $fw -Destination (Join-Path $stage "firmware")
    $renderer = Join-Path $root "clawd_desktop\renderer"
    if (Test-Path -LiteralPath (Join-Path $renderer "preview.json")) {
        Copy-Item -Recurse -LiteralPath $renderer -Destination (Join-Path $stage "renderer")
    } else {
        Write-Host "no clock-screen renderer built (tools/build_preview.py); the preview will be approximate"
    }
    $isccArgs += "/DResourcesDir=$stage"
}

$iscc = (Get-Command iscc -ErrorAction SilentlyContinue).Source
if (-not $iscc) { $iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" }
if (-not (Test-Path -LiteralPath $iscc)) { throw "Inno Setup 6 (iscc) not found" }
$isccArgs += "packaging-rs\windows\clawd.iss"
Invoke-Native "iscc" { & $iscc @isccArgs }

$setup = "dist\Clawd-$version-windows-setup.exe"
if (-not (Test-Path -LiteralPath $setup)) { throw "iscc did not produce $setup" }
Write-Host ("built {0} ({1:N0} bytes)" -f $setup, (Get-Item -LiteralPath $setup).Length)
