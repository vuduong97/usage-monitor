<#
.SYNOPSIS
  Repeatable performance measurement of a tray app on Windows (Python Clawd 0.1.x or the Tauri spike).

.DESCRIPTION
  Launches the app in an isolated profile (own USERPROFILE/HOME/APPDATA/LOCALAPPDATA/TEMP, scrubbed
  token variables, preseeded desktop.json so the Python app never touches HKCU\...\Run), then
  measures: installer_bytes, idle_private_mb_10s, idle_private_mb_10m, tray_ms, paint_cold_ms,
  paint_warm_ms, rss_after_50_cycles_mb. Prints one JSON document; -Out also writes it to a file.

  Definitions (see README.md):
    idle_private_*  private working set of the main process (what Task Manager calls Memory),
                    with 0 windows ever opened; private bytes (commit) and working set are in "runs".
    tray_ms         launch (just before CreateProcess) -> a tray window of the process exists.
    paint_*_ms      trigger (tray click equivalent) -> first frame with page content in the window,
                    seen from outside by PrintWindow of the client area. cold = first window since
                    launch, warm = median of the later open/close cycles.
    rss_after_50    private working set of the main process 5 s after the last of 50 open/close cycles.

.EXAMPLE
  # Python baseline (prebuilt PyInstaller app)
  .\measure.ps1 -Kind python -Exe D:\vu\usage-monitor\dist\Clawd\Clawd.exe -Installer .\Clawd-0.1.4-windows-setup.exe -Runs 3 -Out py.json
.EXAMPLE
  # Tauri spike, 3 parallel 10-minute idle runs, then merge with a window-phase file
  .\measure.ps1 -Kind tauri -Exe .\clawd-spike.exe -Phase idle -Parallel -Runs 3 -Out idle.json
  .\measure.ps1 -Kind tauri -Exe .\clawd-spike.exe -Phase window -Runs 3 -Out window.json
  .\measure.ps1 -Merge idle.json,window.json -Out all.json
#>
[CmdletBinding()]
param(
    [string]$Exe,
    [ValidateSet('tauri', 'python')][string]$Kind = 'tauri',
    [string]$Installer,
    [int]$Runs = 3,
    [int]$Cycles = 50,
    [int]$IdleSeconds = 600,
    [int]$GapMs = 300,
    [ValidateSet('all', 'idle', 'window')][string]$Phase = 'all',
    [switch]$Parallel,
    [switch]$TrimAfterDestroy,
    [string[]]$Merge,
    [string]$Out,
    [string]$WorkDir = (Join-Path ([IO.Path]::GetTempPath()) 'clawd-perf'),
    [int]$BasePort = 47940,
    [string]$TrayClassRegex = 'SystemTrayIcon|tray_icon',
    [string]$DumpFramesDir,
    [hashtable]$ExtraEnv = @{}
)

$ErrorActionPreference = 'Stop'

$source = @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.Net.Sockets;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;

public static class Probe
{
    delegate bool EnumProc(IntPtr h, IntPtr l);
    [StructLayout(LayoutKind.Sequential)] struct RECT { public int L, T, R, B; }
    [StructLayout(LayoutKind.Sequential)] struct POINT { public int X, Y; }

    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string title);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder sb, int n);
    [DllImport("user32.dll")] static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
    [DllImport("kernel32.dll")] static extern void GetSystemTimePreciseAsFileTime(out long ft);

    static Probe() { SetProcessDPIAware(); }

    // Debug aid: when set, the first frame judged "painted" of each window is saved there as PNG.
    public static string DumpDir = null;

    // Wall clock in unix ms with sub-ms resolution: the same clock the spike stamps its log with.
    public static double NowMs()
    {
        long ft; GetSystemTimePreciseAsFileTime(out ft);
        return (ft - 116444736000000000L) / 10000.0;
    }

    static string ClassOf(IntPtr h)
    {
        var sb = new StringBuilder(256);
        GetClassName(h, sb, 256);
        return sb.ToString();
    }

    static uint PidOf(IntPtr h) { uint pid; GetWindowThreadProcessId(h, out pid); return pid; }

    // Class name of the first window of the process (top-level or message-only) matching rx, else null.
    public static string FindTrayWindow(uint pid, string rx)
    {
        var re = new Regex(rx);
        string found = null;
        EnumWindows((h, l) =>
        {
            if (PidOf(h) == pid) { var c = ClassOf(h); if (re.IsMatch(c)) { found = c; return false; } }
            return true;
        }, IntPtr.Zero);
        if (found != null) return found;
        IntPtr hwndMessage = new IntPtr(-3), prev = IntPtr.Zero;
        while ((prev = FindWindowEx(hwndMessage, prev, null, null)) != IntPtr.Zero)
            if (PidOf(prev) == pid) { var c = ClassOf(prev); if (re.IsMatch(c)) return c; }
        return null;
    }

    // Every window class the process owns, for diagnosing -TrayClassRegex.
    public static string[] WindowClasses(uint pid)
    {
        var list = new List<string>();
        EnumWindows((h, l) => { if (PidOf(h) == pid) list.Add("top:" + ClassOf(h) + (IsWindowVisible(h) ? ":visible" : ":hidden")); return true; }, IntPtr.Zero);
        IntPtr hwndMessage = new IntPtr(-3), prev = IntPtr.Zero;
        while ((prev = FindWindowEx(hwndMessage, prev, null, null)) != IntPtr.Zero)
            if (PidOf(prev) == pid) list.Add("msg:" + ClassOf(prev));
        return list.ToArray();
    }

    // ms after t0 when a window of pid exists, or -1 on timeout.
    public static double WaitForTray(uint pid, string rx, int timeoutMs, double t0)
    {
        while (NowMs() - t0 < timeoutMs)
        {
            if (FindTrayWindow(pid, rx) != null) return NowMs() - t0;
            Thread.Sleep(2);
        }
        return -1;
    }

    static IntPtr FindBigWindow(uint pid)
    {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) =>
        {
            RECT r;
            if (PidOf(h) == pid && IsWindowVisible(h) && GetWindowRect(h, out r) && r.R - r.L >= 400 && r.B - r.T >= 300)
            { found = h; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    // True when the client area holds page content: after quantising to 5 bits per channel, more than
    // 0.4% of the sampled pixels differ from the dominant colour (an empty window is one flat colour).
    static bool ClientHasContent(IntPtr h)
    {
        RECT wr, cr; POINT origin = new POINT();
        if (!GetWindowRect(h, out wr)) return false;
        int w = wr.R - wr.L, ht = wr.B - wr.T;
        if (w <= 0 || ht <= 0) return false;
        using (var bmp = new Bitmap(w, ht, PixelFormat.Format32bppRgb))
        {
            using (var g = Graphics.FromImage(bmp))
            {
                IntPtr hdc = g.GetHdc();
                bool ok = PrintWindow(h, hdc, 2); // PW_RENDERFULLCONTENT: includes DirectComposition (WebView2)
                g.ReleaseHdc(hdc);
                if (!ok) return false;
            }
            GetClientRect(h, out cr);
            ClientToScreen(h, ref origin);
            int cx = Math.Max(0, origin.X - wr.L), cy = Math.Max(0, origin.Y - wr.T);
            int cw = Math.Min(cr.R, w - cx), ch = Math.Min(cr.B, ht - cy);
            if (cw <= 0 || ch <= 0) return false;
            var data = bmp.LockBits(new Rectangle(cx, cy, cw, ch), ImageLockMode.ReadOnly, PixelFormat.Format32bppRgb);
            try
            {
                var counts = new Dictionary<int, int>();
                int total = 0;
                var row = new byte[data.Stride];
                for (int y = 0; y < ch; y += 3)
                {
                    Marshal.Copy(new IntPtr(data.Scan0.ToInt64() + (long)y * data.Stride), row, 0, data.Stride);
                    for (int x = 0; x < cw; x += 3)
                    {
                        int i = x * 4;
                        int key = ((row[i] >> 3) << 10) | ((row[i + 1] >> 3) << 5) | (row[i + 2] >> 3);
                        int n; counts.TryGetValue(key, out n); counts[key] = n + 1; total++;
                    }
                }
                int top = 0; foreach (var v in counts.Values) if (v > top) top = v;
                bool painted = total > 0 && (total - top) > total * 0.004;
                if (painted && DumpDir != null) { bmp.UnlockBits(data); data = null; bmp.Save(System.IO.Path.Combine(DumpDir, "paint-" + DateTime.UtcNow.Ticks + ".png"), ImageFormat.Png); }
                return painted;
            }
            finally { if (data != null) bmp.UnlockBits(data); }
        }
    }

    // {ms until a big visible window exists, ms until it shows page content} after t0; -1 = timeout.
    public static double[] WaitForPaint(uint pid, int timeoutMs, double t0)
    {
        double visibleAt = -1;
        IntPtr h = IntPtr.Zero;
        while (NowMs() - t0 < timeoutMs)
        {
            if (h == IntPtr.Zero)
            {
                h = FindBigWindow(pid);
                if (h != IntPtr.Zero) visibleAt = NowMs() - t0;
            }
            if (h != IntPtr.Zero && ClientHasContent(h)) return new[] { visibleAt, NowMs() - t0 };
            Thread.Sleep(1);
        }
        return new[] { visibleAt, -1.0 };
    }

    // One HTTP/1.1 GET on 127.0.0.1; returns the unix ms taken right before the request is written.
    public static double HttpGet(int port, string path, out string status)
    {
        using (var c = new TcpClient())
        {
            c.NoDelay = true;
            c.Connect("127.0.0.1", port);
            var s = c.GetStream();
            var req = Encoding.ASCII.GetBytes("GET " + path + " HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
            double t = NowMs();
            s.Write(req, 0, req.Length);
            var buf = new byte[4096]; int n, got = 0; var sb = new StringBuilder();
            s.ReadTimeout = 20000;
            while ((n = s.Read(buf, 0, buf.Length)) > 0 && got < 4096) { sb.Append(Encoding.ASCII.GetString(buf, 0, n)); got += n; }
            var line = sb.ToString().Split('\n')[0].Trim();
            status = line;
            return t;
        }
    }

    public static bool PortOpen(int port)
    {
        try { using (var c = new TcpClient()) { c.Connect("127.0.0.1", port); return true; } }
        catch (SocketException) { return false; }
    }
}
'@

Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition $source

# ----------------------------------------------------------------------------- helpers

if ($DumpFramesDir) { New-Item -ItemType Directory -Force -Path $DumpFramesDir | Out-Null; [Probe]::DumpDir = (Resolve-Path $DumpFramesDir).Path }

function Get-Median([double[]]$v) {
    if (-not $v -or $v.Count -eq 0) { return $null }
    $s = $v | Sort-Object
    $n = $s.Count
    if ($n % 2) { return [double]$s[($n - 1) / 2] }
    return ([double]$s[$n / 2 - 1] + [double]$s[$n / 2]) / 2
}

function Get-Spread([double[]]$v) {
    if (-not $v -or $v.Count -eq 0) { return $null }
    $s = $v | Sort-Object
    [ordered]@{
        median = [math]::Round((Get-Median $v), 2)
        min    = [math]::Round([double]$s[0], 2)
        max    = [math]::Round([double]$s[-1], 2)
        n      = $v.Count
        values = @($v | ForEach-Object { [math]::Round($_, 2) })
    }
}

function Get-Load {
    $cpu = (Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average
    $builders = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '^(cargo|rustc|link|cl|cc1|cmake|ninja|msbuild|nasm|lld-link)$' }).Count
    [ordered]@{ cpu_pct = $cpu; build_like_procs = $builders }
}

function Get-Mem([int]$ProcessId) {
    $p = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if (-not $p) { return $null }
    $perf = Get-CimInstance Win32_PerfRawData_PerfProc_Process -Filter "IDProcess=$ProcessId"
    [ordered]@{
        private_ws_mb    = [math]::Round($perf.WorkingSetPrivate / 1MB, 2)
        private_bytes_mb = [math]::Round($p.PrivateMemorySize64 / 1MB, 2)
        ws_mb            = [math]::Round($p.WorkingSet64 / 1MB, 2)
    }
}

# msedgewebview2.exe processes that belong to this run: their command line carries the run's
# isolated user-data dir (other apps' WebView2 processes on the machine are never counted). Matched on
# the unique directory name, because %TEMP% may be spelled as an 8.3 short path.
function Get-WebView2([string]$RunDir) {
    $leaf = Split-Path $RunDir -Leaf
    @(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" |
        Where-Object { $_.CommandLine -and $_.CommandLine.IndexOf($leaf, [StringComparison]::OrdinalIgnoreCase) -ge 0 })
}

function Get-WebView2PrivateMb([string]$RunDir) {
    $sum = 0.0
    foreach ($w in Get-WebView2 $RunDir) {
        $perf = Get-CimInstance Win32_PerfRawData_PerfProc_Process -Filter "IDProcess=$($w.ProcessId)"
        if ($perf) { $sum += $perf.WorkingSetPrivate / 1MB }
    }
    [math]::Round($sum, 2)
}

# Wait until no WebView2 process of the run is left; returns ms waited, or -1 when some are still there.
function Wait-WebView2Gone([string]$RunDir, [int]$TimeoutMs = 15000) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.ElapsedMilliseconds -lt $TimeoutMs) {
        if ((Get-WebView2 $RunDir).Count -eq 0) { return $sw.ElapsedMilliseconds }
        Start-Sleep -Milliseconds 100
    }
    return -1
}

function Get-ScrubbedEnv {
    # Real credentials must never reach the app under test.
    $drop = '(TOKEN|SECRET|PASSWORD|PASSWD|CREDENTIAL|API_?KEY|ANTHROPIC|CLAUDE|OPENAI|CODEX|GITHUB|^GH_|AWS_|AZURE_|NPM_)'
    $map = @{}
    foreach ($e in [Environment]::GetEnvironmentVariables().GetEnumerator()) {
        if ($e.Key -notmatch $drop) { $map[$e.Key] = [string]$e.Value }
    }
    $map
}

function New-RunContext([int]$Index) {
    $dir = Join-Path $WorkDir ("{0}-{1}-{2:yyyyMMddHHmmss}" -f $Kind, $Index, (Get-Date))
    $dirs = @{ home = 'home'; local = 'local'; roaming = 'roaming'; temp = 'tmp' }
    foreach ($d in $dirs.Values) { New-Item -ItemType Directory -Force -Path (Join-Path $dir $d) | Out-Null }
    $envMap = Get-ScrubbedEnv
    $homeDir = Join-Path $dir 'home'
    $envMap['USERPROFILE'] = $homeDir
    $envMap['HOME'] = $homeDir
    $envMap['HOMEDRIVE'] = (Split-Path $homeDir -Qualifier)
    $envMap['HOMEPATH'] = $homeDir.Substring(2)
    $envMap['LOCALAPPDATA'] = Join-Path $dir 'local'
    $envMap['APPDATA'] = Join-Path $dir 'roaming'
    $envMap['TEMP'] = Join-Path $dir 'tmp'
    $envMap['TMP'] = Join-Path $dir 'tmp'
    $port = $BasePort + $Index
    $ctx = [ordered]@{ index = $Index; dir = $dir; env = $envMap; port = $port; proc = $null; log = (Join-Path $dir 'spike.log') }
    if ($Kind -eq 'python') {
        $envMap['CLAWD_EVENT_PORT'] = "$port"
        # A fresh profile would make the tray enable autostart (writes the real HKCU Run key) and open the
        # setup wizard: preseed the prefs so it does neither.
        $cfg = Join-Path $homeDir '.config\claude-usage-monitor'
        New-Item -ItemType Directory -Force -Path $cfg | Out-Null
        $prefs = '{"first_run_done": true, "setup_done": ["clock", "agents", "usage"], "setup_shown": "0.1.4"}'
        [IO.File]::WriteAllText((Join-Path $cfg 'desktop.json'), $prefs)
    } else {
        $envMap['CLAWD_SPIKE_PORT'] = "$port"
        $envMap['CLAWD_SPIKE_CONTROL'] = '1'
        $envMap['CLAWD_SPIKE_LOG'] = $ctx.log
        if ($TrimAfterDestroy) { $envMap['CLAWD_SPIKE_TRIM'] = '1' }
    }
    foreach ($k in $ExtraEnv.Keys) { $envMap[[string]$k] = [string]$ExtraEnv[$k] }
    [pscustomobject]$ctx
}

function New-StartInfo([string[]]$Arguments, $Ctx, [bool]$Redirect) {
    $psi = New-Object Diagnostics.ProcessStartInfo
    $psi.FileName = (Resolve-Path $Exe).Path
    $psi.Arguments = ($Arguments | ForEach-Object { if ($_ -match '[\s"]' -or $_ -eq '') { '"' + ($_ -replace '"', '\"') + '"' } else { $_ } }) -join ' '
    $psi.UseShellExecute = $false
    $psi.WorkingDirectory = $Ctx.dir
    $psi.EnvironmentVariables.Clear()
    foreach ($k in $Ctx.env.Keys) { $psi.EnvironmentVariables[$k] = $Ctx.env[$k] }
    if ($Redirect) { $psi.RedirectStandardInput = $true; $psi.RedirectStandardOutput = $true; $psi.RedirectStandardError = $true }
    $psi
}

# Launch the app; returns the context with .proc and .t_launch and the tray result.
function Start-App($Ctx, [bool]$MeasureTray) {
    $psi = New-StartInfo @() $Ctx $false
    $Ctx | Add-Member -NotePropertyName t_launch -NotePropertyValue ([Probe]::NowMs()) -Force
    $Ctx.proc = [Diagnostics.Process]::Start($psi)
    $tray = [Probe]::WaitForTray([uint32]$Ctx.proc.Id, $TrayClassRegex, 20000, $Ctx.t_launch)
    $Ctx | Add-Member -NotePropertyName tray_ms -NotePropertyValue $(if ($MeasureTray) { $tray } else { $null }) -Force
    $Ctx | Add-Member -NotePropertyName tray_found -NotePropertyValue ($tray -ge 0) -Force
    $Ctx | Add-Member -NotePropertyName window_classes -NotePropertyValue ([Probe]::WindowClasses([uint32]$Ctx.proc.Id)) -Force
    if ($Kind -eq 'tauri') {
        $deadline = [DateTime]::UtcNow.AddSeconds(20)
        while (-not [Probe]::PortOpen($Ctx.port) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 20 }
    }
}

function Stop-App($Ctx) {
    if ($Ctx.proc -and -not $Ctx.proc.HasExited) { try { $Ctx.proc.Kill() } catch { } ; $Ctx.proc.WaitForExit(5000) | Out-Null }
    Get-WebView2 $Ctx.dir | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}

# The spike's own log as [ts_ms, event] pairs.
function Read-SpikeLog($Ctx) {
    $list = New-Object System.Collections.Generic.List[object]
    if (Test-Path $Ctx.log) {
        foreach ($line in Get-Content $Ctx.log) {
            $sp = $line.IndexOf(' ')
            if ($sp -gt 0) { $list.Add(@([double]$line.Substring(0, $sp), $line.Substring($sp + 1))) }
        }
    }
    , $list.ToArray()
}

# Events of the n-th open (0 = first) as "event=+ms after open_requested" strings.
function Get-Timeline($Log, [int]$Nth) {
    $starts = @(0..($Log.Count - 1) | Where-Object { $Log[$_][1] -eq 'open_requested' })
    if ($starts.Count -le $Nth) { return @() }
    $from = $starts[$Nth]
    $to = $(if ($starts.Count -gt $Nth + 1) { $starts[$Nth + 1] - 1 } else { $Log.Count - 1 })
    $t0 = $Log[$from][0]
    @($from..$to | ForEach-Object { '{0}=+{1:N0}' -f $Log[$_][1], ($Log[$_][0] - $t0) })
}

# Opens the settings window the way the tray click does and returns the paint timings.
# Returns @{ visible_ms; paint_ms; window_pid; settings = process (python) }.
function Open-Settings($Ctx) {
    if ($Kind -eq 'tauri') {
        $status = $null
        $t0 = [Probe]::HttpGet($Ctx.port, '/open', [ref]$status)
        $r = [Probe]::WaitForPaint([uint32]$Ctx.proc.Id, 15000, $t0)
        return @{ t0 = $t0; visible_ms = $r[0]; paint_ms = $r[1]; window_pid = $Ctx.proc.Id; settings = $null }
    }
    $psi = New-StartInfo @('settings', '--piped', '--clock', '', '--key', '') $Ctx $true
    $t0 = [Probe]::NowMs()
    $p = [Diagnostics.Process]::Start($psi)
    $p.BeginOutputReadLine(); $p.BeginErrorReadLine()
    $r = [Probe]::WaitForPaint([uint32]$p.Id, 20000, $t0)
    return @{ t0 = $t0; visible_ms = $r[0]; paint_ms = $r[1]; window_pid = $p.Id; settings = $p }
}

# Closes it like the X button: tauri window.close() (CloseRequested, then destroy), python CloseMainWindow().
function Close-Settings($Ctx, $Opened) {
    if ($Kind -eq 'tauri') {
        $status = $null
        [void][Probe]::HttpGet($Ctx.port, '/close', [ref]$status)
        return
    }
    $p = $Opened.settings
    if ($p -and -not $p.HasExited) {
        [void]$p.CloseMainWindow()
        if (-not $p.WaitForExit(5000)) { $p.Kill(); $p.WaitForExit(5000) | Out-Null }
    }
}

function Measure-WindowPhase($Ctx, [int]$CycleCount) {
    $res = [ordered]@{}
    # cold: first window since launch
    $o = Open-Settings $Ctx
    $res.paint_cold_ms = $o.paint_ms
    $res.visible_cold_ms = $o.visible_ms
    if ($Kind -eq 'tauri') {
        Start-Sleep -Milliseconds 500
        $res.webview2_private_ws_mb_open = Get-WebView2PrivateMb $Ctx.dir
        $res.webview2_procs_open = (Get-WebView2 $Ctx.dir).Count
    }
    Start-Sleep -Milliseconds 1500
    Close-Settings $Ctx $o
    $res.wv2_exit_ms_cold = Wait-WebView2Gone $Ctx.dir
    Start-Sleep -Seconds 3
    $res.after_first_close = Get-Mem $Ctx.proc.Id
    # warm: the cycles
    $warm = New-Object System.Collections.Generic.List[double]
    $exit = New-Object System.Collections.Generic.List[double]
    $trace = New-Object System.Collections.Generic.List[object]
    $leaked = 0
    for ($i = 1; $i -le $CycleCount; $i++) {
        $o = Open-Settings $Ctx
        if ($o.paint_ms -ge 0) { $warm.Add($o.paint_ms) }
        Start-Sleep -Milliseconds 800
        Close-Settings $Ctx $o
        $w = Wait-WebView2Gone $Ctx.dir
        if ($w -ge 0) { $exit.Add($w) } else { $leaked++ }
        Start-Sleep -Milliseconds $GapMs
        if ($i % 10 -eq 0 -and $i -lt $CycleCount) {
            $m = Get-Mem $Ctx.proc.Id
            $trace.Add([ordered]@{ cycle = $i; private_ws_mb = $m.private_ws_mb; private_bytes_mb = $m.private_bytes_mb })
        }
    }
    Start-Sleep -Seconds 5
    $res.paint_warm_values_ms = @($warm | ForEach-Object { [math]::Round($_, 1) })
    $res.paint_warm_ms = $(if ($warm.Count) { [math]::Round((Get-Median $warm.ToArray()), 1) } else { $null })
    $res.wv2_exit_ms_median = $(if ($exit.Count) { [math]::Round((Get-Median $exit.ToArray()), 0) } else { $null })
    $res.wv2_cycles_not_exited = $leaked
    $res.wv2_procs_after_cycles = (Get-WebView2 $Ctx.dir).Count
    $res.trace_every_10 = $trace.ToArray()
    $res.after_cycles = Get-Mem $Ctx.proc.Id
    $res.rss_after_50_cycles_mb = $res.after_cycles.private_ws_mb
    if ($Kind -eq 'tauri') {
        # SetProcessWorkingSetSize(-1,-1) experiment on the settled, window-less process.
        $status = $null
        [void][Probe]::HttpGet($Ctx.port, '/trim', [ref]$status)
        Start-Sleep -Milliseconds 500
        $res.trim_after = Get-Mem $Ctx.proc.Id
        Start-Sleep -Seconds 10
        $res.trim_after_10s = Get-Mem $Ctx.proc.Id
    }
    $res
}

function Measure-IdlePhase($Ctx, [int]$Seconds) {
    $res = [ordered]@{}
    $wait = $Ctx.t_launch + 10000 - [Probe]::NowMs()
    if ($wait -gt 0) { Start-Sleep -Milliseconds ([int]$wait) }
    $res.idle_10s = Get-Mem $Ctx.proc.Id
    if ($Seconds -gt 10) {
        $wait = $Ctx.t_launch + $Seconds * 1000 - [Probe]::NowMs()
        if ($wait -gt 0) { Start-Sleep -Milliseconds ([int]$wait) }
        $res.idle_10m = Get-Mem $Ctx.proc.Id
    }
    $res
}

function Summarize($RunList) {
    $pick = {
        param($name, $sel)
        $vals = @($RunList | ForEach-Object { & $sel $_ } | Where-Object { $null -ne $_ -and $_ -ge 0 } | ForEach-Object { [double]$_ })
        Get-Spread $vals
    }
    $spread = [ordered]@{
        idle_private_mb_10s         = & $pick 'a' { param($r) $r.idle_10s.private_ws_mb }
        idle_private_mb_10m         = & $pick 'b' { param($r) $r.idle_10m.private_ws_mb }
        idle_private_bytes_mb_10s   = & $pick 'c' { param($r) $r.idle_10s.private_bytes_mb }
        idle_private_bytes_mb_10m   = & $pick 'd' { param($r) $r.idle_10m.private_bytes_mb }
        tray_ms                     = & $pick 'e' { param($r) $r.tray_ms }
        paint_cold_ms               = & $pick 'f' { param($r) $r.window.paint_cold_ms }
        paint_warm_ms               = & $pick 'g' { param($r) $r.window.paint_warm_ms }
        rss_after_50_cycles_mb      = & $pick 'h' { param($r) $r.window.rss_after_50_cycles_mb }
    }
    $summary = [ordered]@{}
    $summary.installer_bytes = $script:InstallerBytes
    foreach ($k in $spread.Keys) { $summary[$k] = $(if ($spread[$k]) { $spread[$k].median } else { $null }) }
    @{ summary = $summary; spread = $spread }
}

# ----------------------------------------------------------------------------- merge mode

if ($Merge) {
    $docs = $Merge | ForEach-Object { Get-Content -Raw $_ | ConvertFrom-Json }
    $script:InstallerBytes = ($docs | Where-Object { $_.summary.installer_bytes } | Select-Object -First 1).summary.installer_bytes
    $mergedRuns = @($docs | ForEach-Object { $_.runs })
    $s = Summarize $mergedRuns
    $first = $docs[0]
    $doc = [ordered]@{
        schema = 1; kind = $first.kind; exe = $first.exe; exe_bytes = $first.exe_bytes
        merged_from = $Merge; machine = $first.machine
        summary = $s.summary; spread = $s.spread; runs = $mergedRuns
    }
    $json = $doc | ConvertTo-Json -Depth 10
    if ($Out) { Set-Content -Path $Out -Value $json -Encoding UTF8 }
    $json
    return
}

# ----------------------------------------------------------------------------- main

if (-not $Exe) { throw '-Exe is required (or use -Merge)' }
$Exe = (Resolve-Path $Exe).Path
New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null
$script:InstallerBytes = $(if ($Installer) { (Get-Item $Installer).Length } else { $null })

$os = Get-CimInstance Win32_OperatingSystem
$machine = [ordered]@{
    os         = "$($os.Caption) $($os.Version)"
    cpu        = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name
    logical    = [Environment]::ProcessorCount
    ram_mb     = [math]::Round($os.TotalVisibleMemorySize / 1024)
    webview2   = (Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' -ErrorAction SilentlyContinue).pv
    load_start = Get-Load
    measured   = (Get-Date).ToString('s')
}

$collected = New-Object System.Collections.Generic.List[object]
$contexts = @()
try {
    if ($Phase -eq 'idle' -and $Parallel) {
        # N independent instances idle side by side; tray_ms is not taken (launches would disturb each other).
        1..$Runs | ForEach-Object { $c = New-RunContext $_; $contexts += $c; Start-App $c $false }
        $byIndex = @{}
        foreach ($c in $contexts) { $byIndex[$c.index] = Measure-IdlePhase $c $IdleSeconds }
        foreach ($c in $contexts) {
            $r = $byIndex[$c.index]
            $entry = [ordered]@{ run = $c.index; mode = 'idle-parallel'; load = (Get-Load); idle_10s = $r.idle_10s; idle_10m = $r.idle_10m }
            if ($Kind -eq 'tauri') {
                $status = $null
                [void][Probe]::HttpGet($c.port, '/trim', [ref]$status)
                Start-Sleep -Milliseconds 500
                $entry.idle_after_trim = Get-Mem $c.proc.Id
            }
            $collected.Add([pscustomobject]$entry)
        }
    } else {
        for ($i = 1; $i -le $Runs; $i++) {
            $c = New-RunContext $i
            $contexts += $c
            $load = Get-Load
            Start-App $c $true
            $entry = [ordered]@{ run = $i; mode = $Phase; load = $load; tray_ms = $c.tray_ms; tray_found = $c.tray_found; tray_window_classes = $c.window_classes }
            if ($Phase -in 'all', 'idle') {
                $idle = Measure-IdlePhase $c $IdleSeconds
                $entry.idle_10s = $idle.idle_10s
                if ($idle.idle_10m) { $entry.idle_10m = $idle.idle_10m }
            } else {
                $idle = Measure-IdlePhase $c 10
                $entry.idle_10s = $idle.idle_10s
            }
            if ($Phase -in 'all', 'window') {
                $entry.load_before_window = Get-Load
                $entry.window = Measure-WindowPhase $c $Cycles
                if ($Kind -eq 'tauri') {
                    $log = Read-SpikeLog $c
                    $t0 = ($log | Where-Object { $_[1] -eq 'process_start' } | Select-Object -First 1)[0]
                    $tr = ($log | Where-Object { $_[1] -eq 'tray_ready' } | Select-Object -First 1)[0]
                    $entry.launch_to_main_ms = [math]::Round($t0 - $c.t_launch, 1)
                    $entry.main_to_tray_ms = [math]::Round($tr - $t0, 1)
                    $entry.timeline_cold = Get-Timeline $log 0
                    $entry.timeline_warm_2nd = Get-Timeline $log 1
                }
            }
            $collected.Add([pscustomobject]$entry)
            Stop-App $c
        }
    }
} finally {
    foreach ($c in $contexts) { Stop-App $c }
}

$machine.load_end = Get-Load
$s = Summarize $collected.ToArray()
$doc = [ordered]@{
    schema = 1; kind = $Kind; exe = $Exe; exe_bytes = (Get-Item $Exe).Length
    phase = $Phase; parallel = [bool]$Parallel; trim_after_destroy = [bool]$TrimAfterDestroy
    cycles = $Cycles; idle_seconds = $IdleSeconds
    machine = $machine; summary = $s.summary; spread = $s.spread; runs = $collected.ToArray()
}
$json = $doc | ConvertTo-Json -Depth 10
if ($Out) { Set-Content -Path $Out -Value $json -Encoding UTF8 }
$json
