<#
.SYNOPSIS
  Reads the filled `.env` (see release-secrets.template.env) and stores the release secrets and
  variables in GitHub with the gh CLI. Dry run unless -Apply is given. Never prints a value.
#>
param(
    [string]$EnvFile = (Join-Path (Split-Path -Parent $PSScriptRoot) '.env'),
    [string]$Repo = 'vuduong97/usage-monitor',
    [string]$Environment = 'release',
    [switch]$Apply
)
$ErrorActionPreference = 'Stop'

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { throw 'gh CLI not found' }
if (-not (Test-Path $EnvFile)) { throw "No ${EnvFile} - copy packaging-rs\release-secrets.template.env to .env and fill it in" }

$v = @{}
foreach ($line in Get-Content $EnvFile) {
    if ($line -match '^\s*#' -or $line -notmatch '=') { continue }
    $k, $val = $line -split '=', 2
    $v[$k.Trim()] = $val.Trim()
}

$problems = @()
foreach ($k in 'MINISIGN_PASSWORD', 'MINISIGN_KEY_FILE', 'MINISIGN_PUBKEY', 'EXPECTED_TEAM_ID') {
    if (-not $v[$k]) { $problems += "$k is empty" }
}
if ($v['MINISIGN_PUBKEY'] -and $v['MINISIGN_PUBKEY'] -notmatch '^RW[A-Za-z0-9+/]{54}$') {
    $problems += 'MINISIGN_PUBKEY must be the base64 public key line (starts with RW, 56 characters), not the file'
}
if ($v['EXPECTED_TEAM_ID'] -and $v['EXPECTED_TEAM_ID'] -notmatch '^[A-Z0-9]{10}$') {
    $problems += 'EXPECTED_TEAM_ID must be 10 characters (A-Z, 0-9)'
}
if ($v['MINISIGN_KEY_FILE'] -and -not (Test-Path $v['MINISIGN_KEY_FILE'])) {
    $problems += 'MINISIGN_KEY_FILE does not exist'
} else {
    # A minisign secret key file: an "untrusted comment:" line, then a base64 line that starts RWRTY0Iy.
    $keyLines = @(Get-Content $v['MINISIGN_KEY_FILE'] -TotalCount 2)
    if ($keyLines.Count -lt 2 -or $keyLines[0] -notmatch '^untrusted comment:' -or $keyLines[1] -notmatch '^RWRTY0Iy') {
        $problems += 'MINISIGN_KEY_FILE does not look like a minisign secret key (expected the encrypted .key file, not the .pub)'
    }
}
if ($problems) { $problems | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }; throw 'fix .env first' }

$plan = @(
    "secret   MINISIGN_KEY      (content of $($v['MINISIGN_KEY_FILE'])) -> environment $Environment",
    "secret   MINISIGN_PASSWORD (hidden)                               -> environment $Environment",
    "variable MINISIGN_PUBKEY   = $($v['MINISIGN_PUBKEY'])             -> repository",
    "variable EXPECTED_TEAM_ID  = $($v['EXPECTED_TEAM_ID'])            -> repository"
)
$plan | ForEach-Object { Write-Host $_ }
if (-not $Apply) { Write-Host "`nDry run. Add -Apply to set them." -ForegroundColor Yellow; return }

Get-Content $v['MINISIGN_KEY_FILE'] -Raw | gh secret set MINISIGN_KEY --repo $Repo --env $Environment
$v['MINISIGN_PASSWORD'] | gh secret set MINISIGN_PASSWORD --repo $Repo --env $Environment
gh variable set MINISIGN_PUBKEY --repo $Repo --body $v['MINISIGN_PUBKEY']
gh variable set EXPECTED_TEAM_ID --repo $Repo --body $v['EXPECTED_TEAM_ID']
Write-Host "`nDone. Delete .env now." -ForegroundColor Green
