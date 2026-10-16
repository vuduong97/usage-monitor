<#
.SYNOPSIS
  READ-ONLY verification of a Clawd install on a Windows machine (phase 14 migration checks).

.DESCRIPTION
  Inspects, never changes:
    * the HKCU Run value "Clawd" (must be "<exe>" with no arguments)
    * Add/Remove Programs entries named Clawd (must be exactly one)
    * the install folder %LOCALAPPDATA%\Programs\Clawd (Clawd.exe version and hash, _internal gone)
    * the config folder %USERPROFILE%\.config\claude-usage-monitor (file hashes, desktop.json key
      names - never values)
    * Credential Manager entries for vn.clawd.desktop (target names only - never the secret)
    * the Claude Code hook entries in ~\.claude\settings.json
    * GET http://127.0.0.1:<port>/state of the running app
    * app-update\ (ready.json / failed marker) and the log (update loop check)

  It refuses to run unless -ConfirmDisposableVm is given, and before doing anything it parses its
  own text and refuses to continue if it contains a command that could write, delete, start or
  stop anything (see Assert-ReadOnly). It writes no file: redirect stdout yourself
  (".\run_windows.ps1 -ConfirmDisposableVm -Json > before.json").

  NEVER run it against a machine whose Clawd install you care about for any reason other than
  reading it: it is read-only, but the migration procedure around it (windows.md) is not.

.PARAMETER ConfirmDisposableVm
  Required. You promise this is the clean Windows VM of tools/migration/windows.md.

.PARAMETER Stage
  Before = the 0.1.4 (Python) install you are about to update; After = after the update to Rust.
  Check  = just report, assert nothing about before/after.

.PARAMETER ExpectVersion
  The version the install must report (Stage After), e.g. 0.2.1.

.PARAMETER BaselineJson
  The -Json output of the Before run. Stage After compares against it.

.PARAMETER Json
  Print the snapshot as JSON (for a Before baseline) instead of the checklist.

.PARAMETER EventPort
  The event server port (default 47913; the real default, only ever queried with GET /state).

.EXAMPLE
  .\run_windows.ps1 -ConfirmDisposableVm -Stage Before -Json > before.json
  .\run_windows.ps1 -ConfirmDisposableVm -Stage After -ExpectVersion 0.2.1 -BaselineJson .\before.json
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][switch]$ConfirmDisposableVm,
    [ValidateSet('Before', 'After', 'Check')][string]$Stage = 'Check',
    [string]$ExpectVersion = '',
    [string]$BaselineJson = '',
    [switch]$Json,
    [int]$EventPort = 47913,
    [int]$LogLines = 2000
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$AppId = '{6C9B4E2A-1F7D-4C3B-9A51-C1A7D0E5B9F3}'
$RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$RunName = 'Clawd'
$KeyringService = 'vn.clawd.desktop'

# --------------------------------------------------------------------------------------------
# Refuse to modify anything: this script must not contain a command that writes, deletes, starts
# or stops. The check reads the script's own AST, so editing in a write makes it refuse to run.
# --------------------------------------------------------------------------------------------
function Assert-ReadOnly {
    $errors = $null
    $tokens = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile($PSCommandPath, [ref]$tokens, [ref]$errors)
    if ($errors -and $errors.Count -gt 0) { throw "run_windows.ps1 does not parse: $($errors[0].Message)" }
    $denied = @(
        'Set-ItemProperty', 'New-ItemProperty', 'Remove-ItemProperty', 'Rename-ItemProperty', 'Clear-ItemProperty',
        'Set-Item', 'New-Item', 'Remove-Item', 'Move-Item', 'Copy-Item', 'Rename-Item', 'Clear-Item',
        'Set-Content', 'Add-Content', 'Clear-Content', 'Out-File', 'Tee-Object', 'Export-Clixml', 'Export-Csv',
        'Start-Process', 'Stop-Process', 'Start-Service', 'Stop-Service', 'Restart-Service', 'Set-Service',
        'Invoke-Expression', 'iex', 'Invoke-Command', 'Set-ExecutionPolicy', 'Register-ScheduledTask',
        'Unregister-ScheduledTask', 'Set-Location', 'Push-Location',
        'reg', 'reg.exe', 'taskkill', 'msiexec', 'schtasks', 'sc', 'sc.exe', 'icacls', 'takeown', 'attrib',
        'cmd', 'cmd.exe', 'powershell', 'powershell.exe', 'pwsh', 'curl', 'curl.exe', 'del', 'rd', 'rmdir', 'erase',
        'ni', 'si', 'ri', 'mi', 'cpi', 'rni', 'sp', 'rp', 'sc', 'ac', 'saps', 'spps', 'kill'
    )
    $bad = New-Object System.Collections.Generic.List[string]
    $commands = $ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.CommandAst] }, $true)
    foreach ($c in $commands) {
        $name = $c.GetCommandName()
        if ($null -eq $name) { $bad.Add("dynamic command at line $($c.Extent.StartLineNumber)"); continue }
        if ($denied -contains $name) { $bad.Add("$name at line $($c.Extent.StartLineNumber)"); continue }
        if ($name -eq 'cmdkey') {
            $first = if ($c.CommandElements.Count -gt 1) { $c.CommandElements[1].Extent.Text } else { '' }
            if ($first -notlike '/list*') { $bad.Add("cmdkey other than /list at line $($c.Extent.StartLineNumber)") }
        }
        if ($name -in @('Invoke-WebRequest', 'Invoke-RestMethod')) {
            $params = @($c.CommandElements | Where-Object { $_ -is [System.Management.Automation.Language.CommandParameterAst] } | ForEach-Object { $_.ParameterName })
            if ($params -contains 'Method' -or $params -contains 'Body' -or $params -contains 'InFile' -or $params -contains 'OutFile') {
                $bad.Add("$name with a write parameter at line $($c.Extent.StartLineNumber)")
            }
        }
    }
    $redirections = $ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.FileRedirectionAst] }, $true)
    foreach ($r in $redirections) { $bad.Add("file redirection at line $($r.Extent.StartLineNumber)") }
    $members = $ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.InvokeMemberExpressionAst] }, $true)
    foreach ($m in $members) {
        $text = $m.Member.Extent.Text
        if ($text -match '^(Delete|Create|Write|Append|Move|Copy|Replace|SetValue|DeleteValue|DeleteSubKey|CreateSubKey|Kill|Start|WriteAll|Open|OpenWrite)') {
            if ($m.Expression.Extent.Text -notmatch 'OpenSubKey') { $bad.Add("$text at line $($m.Extent.StartLineNumber)") }
        }
    }
    if ($bad.Count -gt 0) { throw "REFUSING TO RUN: this script must stay read-only. Found: $($bad -join '; ')" }
}

# --------------------------------------------------------------------------------------------
# Readers
# --------------------------------------------------------------------------------------------
function Get-Sha256([string]$Path) {
    if (Test-Path -LiteralPath $Path -PathType Leaf) { return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
    return $null
}

function Get-RunValue {
    $p = Get-ItemProperty -LiteralPath $RunKey -ErrorAction SilentlyContinue
    if ($null -ne $p -and ($p.PSObject.Properties.Name -contains $RunName)) { return [string]$p.$RunName }
    return $null
}

function Get-UninstallEntries {
    $roots = @(
        'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall',
        'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'
    )
    $found = @()
    foreach ($root in $roots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        foreach ($k in Get-ChildItem -LiteralPath $root -ErrorAction SilentlyContinue) {
            $p = Get-ItemProperty -LiteralPath $k.PSPath -ErrorAction SilentlyContinue
            if ($null -eq $p) { continue }
            $name = if ($p.PSObject.Properties.Name -contains 'DisplayName') { [string]$p.DisplayName } else { '' }
            if ($name -like 'Clawd*' -or $k.PSChildName -like "*$AppId*") {
                $found += [pscustomobject]@{
                    key            = $k.Name
                    displayName    = $name
                    displayVersion = if ($p.PSObject.Properties.Name -contains 'DisplayVersion') { [string]$p.DisplayVersion } else { '' }
                    installLocation = if ($p.PSObject.Properties.Name -contains 'InstallLocation') { [string]$p.InstallLocation } else { '' }
                }
            }
        }
    }
    return , $found
}

function Get-CredentialTargets {
    # Names only. `cmdkey /list` never prints a secret.
    $lines = & cmdkey /list
    return , @($lines | Where-Object { $_ -match [regex]::Escape($KeyringService) } | ForEach-Object { $_.Trim() })
}

function Get-HookEntries {
    $settings = Join-Path $env:USERPROFILE '.claude\settings.json'
    $info = [ordered]@{ file = $settings; exists = (Test-Path -LiteralPath $settings); clawdHooks = 0; sha256 = $null }
    if ($info.exists) {
        $info.sha256 = Get-Sha256 $settings
        $text = Get-Content -LiteralPath $settings -Raw
        $info.clawdHooks = ([regex]::Matches($text, '/hook/claude|clawd-event')).Count
    }
    return [pscustomobject]$info
}

function Get-StateProbe([int]$Port) {
    $uri = "http://127.0.0.1:$Port/state"
    try {
        $r = Invoke-WebRequest -UseBasicParsing -Uri $uri -TimeoutSec 3
        $keys = @((ConvertFrom-Json $r.Content).PSObject.Properties.Name)
        return [pscustomobject]@{ uri = $uri; status = [int]$r.StatusCode; keys = $keys }
    }
    catch {
        return [pscustomobject]@{ uri = $uri; status = 0; keys = @(); error = $_.Exception.Message }
    }
}

function Get-Snapshot {
    $install = Join-Path $env:LOCALAPPDATA 'Programs\Clawd'
    $exe = Join-Path $install 'Clawd.exe'
    $cfg = Join-Path $env:USERPROFILE '.config\claude-usage-monitor'
    $item = if (Test-Path -LiteralPath $exe) { Get-Item -LiteralPath $exe } else { $null }
    $names = if (Test-Path -LiteralPath $install) { @(Get-ChildItem -LiteralPath $install -Force | ForEach-Object { $_.Name }) } else { @() }

    $files = [ordered]@{}
    foreach ($n in 'desktop.json', 'clock.json', 'config', 'pet.json') {
        $f = Join-Path $cfg $n
        $files[$n] = [pscustomobject]@{ exists = (Test-Path -LiteralPath $f); sha256 = (Get-Sha256 $f) }
    }
    $desktopKeys = @()
    $usageSource = $null
    $desktop = Join-Path $cfg 'desktop.json'
    if (Test-Path -LiteralPath $desktop) {
        try {
            $d = Get-Content -LiteralPath $desktop -Raw | ConvertFrom-Json
            $desktopKeys = @($d.PSObject.Properties.Name)
            if ($desktopKeys -contains 'usage_source') { $usageSource = [string]$d.usage_source }
        }
        catch { $desktopKeys = @('<<desktop.json is not valid JSON>>') }
    }
    $stage = Join-Path $cfg 'app-update'
    $stageNames = if (Test-Path -LiteralPath $stage) { @(Get-ChildItem -LiteralPath $stage -Force | ForEach-Object { $_.Name }) } else { @() }
    $logFile = Join-Path $cfg 'clawd-desktop.log'
    $applyLines = 0
    $logTail = @()
    if (Test-Path -LiteralPath $logFile) {
        $logTail = @(Get-Content -LiteralPath $logFile -Tail $LogLines)
        $applyLines = @($logTail | Where-Object { $_ -match 'applying [0-9]' }).Count
    }
    $procs = @(Get-Process -Name Clawd -ErrorAction SilentlyContinue | ForEach-Object { [pscustomobject]@{ id = $_.Id; path = $_.Path } })

    return [pscustomobject]@{
        takenAt        = (Get-Date).ToString('o')
        installDir     = $install
        exeExists      = [bool]$item
        exeProductVersion = if ($item) { [string]$item.VersionInfo.ProductVersion } else { $null }
        exeFileVersion = if ($item) { [string]$item.VersionInfo.FileVersion } else { $null }
        exeSha256      = (Get-Sha256 $exe)
        exeBytes       = if ($item) { $item.Length } else { 0 }
        installNames   = $names
        internalPresent = ($names -contains '_internal')
        runValue       = (Get-RunValue)
        uninstall      = (Get-UninstallEntries)
        configDir      = $cfg
        configFiles    = [pscustomobject]$files
        desktopKeys    = $desktopKeys
        usageSource    = $usageSource
        credentials    = (Get-CredentialTargets)
        hooks          = (Get-HookEntries)
        state          = (Get-StateProbe $EventPort)
        appUpdate      = [pscustomobject]@{ names = $stageNames; failedMarker = ($stageNames -contains 'failed'); ready = ($stageNames -contains 'ready.json') }
        applyLogLines  = $applyLines
        processes      = $procs
    }
}

# --------------------------------------------------------------------------------------------
# Checklist
# --------------------------------------------------------------------------------------------
$script:fails = 0
$script:warns = 0
function Report([string]$Level, [string]$Id, [string]$Text) {
    if ($Level -eq 'FAIL') { $script:fails++ }
    if ($Level -eq 'WARN') { $script:warns++ }
    Write-Output ('{0,-5} {1,-22} {2}' -f $Level, $Id, $Text)
}
function Expect([bool]$Ok, [string]$Id, [string]$Text) {
    if ($Ok) { Report 'PASS' $Id $Text } else { Report 'FAIL' $Id $Text }
}

function Invoke-Checklist($Now, $Base) {
    $expectedRun = '"' + $Now.installDir + '\Clawd.exe"'
    Expect $Now.exeExists 'exe' "Clawd.exe exists in $($Now.installDir)"
    Report 'INFO' 'exe-version' "ProductVersion=$($Now.exeProductVersion) FileVersion=$($Now.exeFileVersion) sha256=$($Now.exeSha256)"
    Expect ($null -ne $Now.runValue) 'run-present' "HKCU Run value $RunName exists"
    Expect ($Now.runValue -ceq $expectedRun) 'run-exact' "Run value is exactly $expectedRun (no arguments); now: $($Now.runValue)"
    $count = @($Now.uninstall).Count
    Expect ($count -eq 1) 'addremove-one' "exactly 1 Add/Remove Programs entry for Clawd (found $count)"
    foreach ($u in $Now.uninstall) { Report 'INFO' 'addremove' "$($u.key) | $($u.displayName) | $($u.displayVersion) | $($u.installLocation)" }
    if ($Stage -eq 'After') {
        Expect (-not $Now.internalPresent) 'no-_internal' 'the PyInstaller folder _internal\ is gone'
        Expect ($Now.installNames -contains 'Clawd.exe') 'files' "install folder holds: $($Now.installNames -join ', ')"
    }
    else {
        Report 'INFO' 'files' "install folder holds: $($Now.installNames -join ', ') (_internal present: $($Now.internalPresent))"
    }
    Report 'INFO' 'prefs-keys' "desktop.json keys: $($Now.desktopKeys -join ', '); usage_source=$($Now.usageSource)"
    foreach ($n in 'desktop.json', 'clock.json', 'config', 'pet.json') {
        $f = $Now.configFiles.$n
        Report 'INFO' "file:$n" "exists=$($f.exists) sha256=$($f.sha256)"
    }
    Expect (@($Now.credentials).Count -ge 1) 'credential' "Credential Manager has an entry for $KeyringService (found $(@($Now.credentials).Count)); names only: $(@($Now.credentials) -join ' | ')"
    Expect ($Now.hooks.clawdHooks -ge 1) 'hooks-installed' "$($Now.hooks.file) has $($Now.hooks.clawdHooks) Clawd hook reference(s)"
    Expect ($Now.state.status -eq 200) 'event-server' "GET $($Now.state.uri) answers 200 (status $($Now.state.status)); keys: $($Now.state.keys -join ',')"
    Expect (@($Now.processes).Count -eq 1) 'one-process' "exactly one Clawd.exe process (found $(@($Now.processes).Count))"
    foreach ($p in $Now.processes) { Report 'INFO' 'process' "pid $($p.id) $($p.path)" }
    Expect (-not $Now.appUpdate.failedMarker) 'no-failed-marker' "app-update\ has no 'failed' marker (contents: $($Now.appUpdate.names -join ', '))"
    Expect ($Now.applyLogLines -le 1) 'no-update-loop' "the last $LogLines log lines mention 'applying <version>' $($Now.applyLogLines) time(s) (more than 1 = an update loop)"

    if ($Stage -eq 'After') {
        if ($ExpectVersion) {
            Expect ($Now.exeProductVersion -like "$ExpectVersion*") 'version-exe' "Clawd.exe ProductVersion starts with $ExpectVersion"
            $uv = @($Now.uninstall | ForEach-Object { $_.displayVersion })
            Expect ($uv -contains $ExpectVersion) 'version-addremove' "Add/Remove DisplayVersion is $ExpectVersion (found: $($uv -join ','))"
        }
        else { Report 'WARN' 'version' 'pass -ExpectVersion to check the installed version' }
        if ($null -eq $Base) { Report 'WARN' 'baseline' 'no -BaselineJson: prefs/Run/credential/hook comparisons skipped' }
        else {
            Expect ($Now.runValue -ceq $Base.runValue) 'run-unchanged' "Run value unchanged since the baseline (before: $($Base.runValue))"
            $lost = @($Base.desktopKeys | Where-Object { $Now.desktopKeys -notcontains $_ })
            Expect ($lost.Count -eq 0) 'prefs-keys-kept' "every desktop.json key of the baseline is still there (lost: $($lost -join ','))"
            Expect ($Now.usageSource -ceq $Base.usageSource) 'usage-source' "usage_source unchanged ($($Base.usageSource) -> $($Now.usageSource))"
            foreach ($n in 'desktop.json', 'clock.json', 'config') {
                $before = $Base.configFiles.$n
                $after = $Now.configFiles.$n
                if ($before.exists -and -not $after.exists) { Report 'FAIL' "file:$n" "$n existed before and is gone" }
                elseif ($before.sha256 -ceq $after.sha256) { Report 'PASS' "file:$n" "$n is byte-identical to the baseline" }
                else { Report 'WARN' "file:$n" "$n changed (the app may rewrite it on start: compare the keys/values by hand)" }
            }
            $credLost = @($Base.credentials | Where-Object { @($Now.credentials) -notcontains $_ })
            Expect ($credLost.Count -eq 0) 'credential-kept' "the keychain entries of the baseline are still there (lost: $($credLost -join ' | '))"
            Expect ($Now.hooks.clawdHooks -ge $Base.hooks.clawdHooks) 'hooks-kept' "Clawd hook references: $($Base.hooks.clawdHooks) -> $($Now.hooks.clawdHooks)"
        }
    }
    Write-Output ''
    Write-Output 'HUMAN (this script cannot see these - see windows.md):'
    Write-Output '  [ ] Settings opens; Wi-Fi/clock tabs show the saved values; the clock password is NOT asked again'
    Write-Output '  [ ] A Claude Code prompt moves the clock (hook works end to end)'
    Write-Output '  [ ] No second update offer after the restart; Settings > App says "up to date"'
    Write-Output '  [ ] Tray icon, menu items, "Thoat" exits the process'
    Write-Output ''
    Write-Output ("RESULT: {0} fail(s), {1} warning(s)" -f $script:fails, $script:warns)
}

# --------------------------------------------------------------------------------------------
Assert-ReadOnly
if ($PSVersionTable.PSVersion.Major -lt 5) { throw 'PowerShell 5.1 or newer is required' }

$now = Get-Snapshot
if ($Json) {
    $now | ConvertTo-Json -Depth 8
    exit 0
}
$base = $null
if ($BaselineJson) {
    if (-not (Test-Path -LiteralPath $BaselineJson)) { throw "baseline not found: $BaselineJson" }
    $base = Get-Content -LiteralPath $BaselineJson -Raw | ConvertFrom-Json
}
Invoke-Checklist $now $base
if ($script:fails -gt 0) { exit 1 }
exit 0
