# Build dist\Clawd\Clawd.exe and dist\Clawd-<version>-windows-setup.exe (run on Windows).
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
$py = if ($env:PYTHON) { $env:PYTHON } else { "3.12" }
uv sync -q --python $py --group build
uv run -q --python $py python packaging/make_icons.py
if (Test-Path dist\Clawd) { Remove-Item -Recurse -Force dist\Clawd }
uv run -q --python $py --group build pyinstaller --noconfirm --clean --log-level WARN --distpath dist --workpath build packaging/clawd.spec
$version = uv run -q --python $py python -c "import clawd_desktop as c; print(c.__version__)"
$iscc = (Get-Command iscc -ErrorAction SilentlyContinue).Source
if (-not $iscc) { $iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" }
& $iscc "/DAppVersion=$version" packaging\windows\clawd.iss
if ($LASTEXITCODE -ne 0) { throw "iscc failed" }
Write-Host "built dist\Clawd-$version-windows-setup.exe"
