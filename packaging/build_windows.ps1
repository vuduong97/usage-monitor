# Build the Rust app and dist\Clawd-<version>-windows-setup.exe (run on Windows).
#   cargo build --release -p clawd-app  ->  Clawd.exe  ->  ISCC (packaging\windows\clawd.iss)
# Build-time settings come from the environment (the release workflow sets them from its secrets):
#   CLAWD_MINISIGN_PUBKEY              minisign public key (SHA256SUMS of app updates, remote
#                                      style packs)
# The clock's firmware ships inside the app: build\firmware and build\renderer from
# tools/build_clock.sh (CI: the firmware job's artifact + tools/build_clock.sh --renderer). A CI
# build without them fails; a local build without them only warns.
#   CARGO_TARGET_DIR                   where cargo puts target\ (default target)
#   PYTHON                             interpreter for check_version.py (default: python)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
$root = (Get-Location).Path

function Invoke-Native([string]$what, [scriptblock]$run) {
    & $run
    if ($LASTEXITCODE -ne 0) { throw "$what failed (exit $LASTEXITCODE)" }
}

$py = if ($env:PYTHON) { $env:PYTHON } else { "python" }
$version = (& $py packaging/check_version.py).Trim()
if ($LASTEXITCODE -ne 0 -or -not $version) { throw "Cargo.toml and tauri.conf.json disagree on the version" }

$cargoArgs = @("build", "--release", "-p", "clawd-app", "--manifest-path", "Cargo.toml",
               "--features", "tauri/custom-protocol")   # what `tauri build` turns on: pages come from the exe
if (Test-Path Cargo.lock) { $cargoArgs += "--locked" }
Invoke-Native "cargo build" { cargo @cargoArgs }

$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }
$exeDir = Join-Path $targetDir "release"
$exe = Join-Path $exeDir "Clawd.exe"
# The installer, the autostart entry and the updater all name Clawd.exe: assert it, exact case.
if (-not (Test-Path -LiteralPath $exe)) { throw "cargo did not produce $exe" }
if ((Get-Item -LiteralPath $exe).Name -cne "Clawd.exe") { throw "binary is not named Clawd.exe: $((Get-Item -LiteralPath $exe).Name)" }
Write-Host ("Clawd.exe: {0:N0} bytes (reported only, not gated)" -f (Get-Item -LiteralPath $exe).Length)

$isccArgs = @("/DAppVersion=$version", "/DExeDir=$exeDir")
$fw = Join-Path $root "build\firmware"
$renderer = Join-Path $root "build\renderer"
$haveFw = (Test-Path -LiteralPath (Join-Path $fw "firmware.json")) -and (Test-Path -LiteralPath (Join-Path $renderer "preview.json"))
if (-not $haveFw) {
    if ($env:GITHUB_ACTIONS -eq "true") { throw "no build\firmware or build\renderer (tools/build_clock.sh): a release always carries the clock firmware" }
    Write-Host "no build\firmware or build\renderer (tools/build_clock.sh): this app carries no clock firmware"
} else {
    Invoke-Native "bundle check" { & $py tools/bundle_firmware.py --check $fw }
    $stage = Join-Path $root "dist\rs-resources"
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    New-Item -ItemType Directory -Path $stage | Out-Null
    Copy-Item -Recurse -LiteralPath $fw -Destination (Join-Path $stage "firmware")
    Copy-Item -Recurse -LiteralPath $renderer -Destination (Join-Path $stage "renderer")
    $isccArgs += "/DResourcesDir=$stage"
}

$iscc = (Get-Command iscc -ErrorAction SilentlyContinue).Source
if (-not $iscc) { $iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" }
if (-not (Test-Path -LiteralPath $iscc)) { throw "Inno Setup 6 (iscc) not found" }
$isccArgs += "packaging\windows\clawd.iss"
Invoke-Native "iscc" { & $iscc @isccArgs }

$setup = "dist\Clawd-$version-windows-setup.exe"
if (-not (Test-Path -LiteralPath $setup)) { throw "iscc did not produce $setup" }
Write-Host ("built {0} ({1:N0} bytes)" -f $setup, (Get-Item -LiteralPath $setup).Length)
