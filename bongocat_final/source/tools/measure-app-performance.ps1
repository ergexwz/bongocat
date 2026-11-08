# Build bongocat-app --release --features storage-test-injection before running.
# Samples an isolated Development store; only its own Tauri panels are closed.
param(
    [ValidateRange(30,28800)][int]$Seconds=90,
    [switch]$Exercise,
    [switch]$RestartPlugins,
    [switch]$KeepAwake,
    [ValidateRange(0,300)][int]$WarmupSeconds=30,
    [ValidatePattern('^[a-zA-Z0-9._-]+$')][string]$Name='short',
    [string]$Executable='',
    [string]$PluginDirectory='',
    [switch]$ResourceOnly
)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'performance-processes.ps1')
$source = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskOutput = Join-Path (Join-Path $source 'target/performance-app') $Name
if (Test-Path -LiteralPath $taskOutput) { throw 'Use a fresh scenario name to preserve previous evidence' }
[void][IO.Directory]::CreateDirectory($taskOutput)
$env:BONGOCAT_PLUGIN_DEV_DIRECTORY = if ($PluginDirectory) { [IO.Path]::GetFullPath($PluginDirectory) } else { Join-Path $source 'target/plugin-dev/plugins' }
if ($ResourceOnly) { Remove-Item Env:BONGOCAT_FRAME_TIMING -ErrorAction SilentlyContinue } else { $env:BONGOCAT_FRAME_TIMING='1' }
$env:BONGOCAT_PERF_REPORT = Join-Path $taskOutput 'frame-report.json'
$env:BONGOCAT_PERF_CONTROL_REPORT = Join-Path $taskOutput 'control-events.jsonl'
if ($Exercise) { $env:BONGOCAT_PERF_EXERCISE='1' } else { Remove-Item Env:BONGOCAT_PERF_EXERCISE -ErrorAction SilentlyContinue }
if ($RestartPlugins) { $env:BONGOCAT_PERF_RESTART_PLUGINS='1' } else { Remove-Item Env:BONGOCAT_PERF_RESTART_PLUGINS -ErrorAction SilentlyContinue }
$taskPrefix=Join-Path $taskOutput 'app'
$profileRoot=Join-Path $taskOutput 'isolated-store'
# WebView2 resolves this relative to each temporary plugin executable, isolating
# its browser profile as well as its business files without changing the plugin.
$env:WEBVIEW2_USER_DATA_FOLDER='.bongocat-profile-webview2'
$appExecutable=if ($Executable) { [IO.Path]::GetFullPath($Executable) } else { Join-Path $source 'target/release/bongocat-app.exe' }
$arguments=@('--performance-smoke-root',('"'+$profileRoot+'"'),'--run-seconds',"$Seconds")
Add-Type -Path (Join-Path $PSScriptRoot 'performance-windows.cs')
$closedWindows=0; $lastClose=0
$taskProcess=Start-Process -FilePath $appExecutable -ArgumentList $arguments -WindowStyle Hidden -PassThru -RedirectStandardOutput "$taskPrefix.stdout.txt" -RedirectStandardError "$taskPrefix.stderr.txt"
    $taskStarted = [DateTime]::UtcNow
    # Keep the native handle so Windows PowerShell retains the exit status.
    [void]$taskProcess.Handle
    $taskClock = [Diagnostics.Stopwatch]::StartNew()
    $taskKnown = [Collections.Generic.HashSet[int]]::new(); [void]$taskKnown.Add($taskProcess.Id)
    $taskCreated = @{}; $taskCpu = @{}
    $skippedProcessSamples = 0
    $taskRows = [Collections.Generic.List[object]]::new()
    $taskCsv = "$taskPrefix.csv"
    try {
    if ($KeepAwake -and [ProfileWindows]::SetThreadExecutionState([uint32]2147483651) -eq 0) { throw 'Cannot keep this bounded graphics test awake' }
    $lastLogs = -60; $taskLogBytes = 0L
    while (-not $taskProcess.HasExited) {
        # Follow descendants by identity, never by executable name or user data path.
        $taskAll = @(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,CreationDate)
        foreach ($taskId in @($taskKnown)) {
            $taskIdentity = $taskAll | Where-Object ProcessId -eq $taskId | Select-Object -First 1
            if ($null -eq $taskIdentity -or ($taskCreated.ContainsKey($taskId) -and $taskCreated[$taskId] -ne $taskIdentity.CreationDate)) { [void]$taskKnown.Remove($taskId); $taskCpu.Remove($taskId); $taskCreated.Remove($taskId) }
            elseif (-not $taskCreated.ContainsKey($taskId)) { $taskCreated[$taskId] = $taskIdentity.CreationDate }
        }
        do { $taskAdded = $false; foreach ($taskChild in $taskAll) { if ((Test-PerformanceDescendant -Child $taskChild -KnownProcesses $taskKnown -CreationTimes $taskCreated) -and -not $taskKnown.Contains([int]$taskChild.ProcessId)) { [void]$taskKnown.Add([int]$taskChild.ProcessId); $taskCreated[[int]$taskChild.ProcessId] = $taskChild.CreationDate; $taskAdded = $true } } } while ($taskAdded)
        $taskElapsed = $taskClock.Elapsed.TotalSeconds
        if ($taskElapsed -gt $Seconds + 45) { throw 'Application exceeded the isolated smoke deadline' }
        $taskReady = $taskElapsed -ge $WarmupSeconds
        if ($taskElapsed - $lastLogs -ge 60) {
            $appLogs = Join-Path $profileRoot 'com.ayangweb.bongo-cat/development/logs'
            $taskLogBytes = [long](Get-ChildItem -LiteralPath $appLogs -File -Filter '*.log' -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum
            $lastLogs = $taskElapsed
        }
        $taskPrivate = @{}; $taskGpu = @{}
        try { foreach ($taskCounter in Get-CimInstance Win32_PerfFormattedData_PerfProc_Process) { $taskPrivate[[int]$taskCounter.IDProcess] = [long]$taskCounter.WorkingSetPrivate } } catch {}
        try {
            foreach ($taskCounter in Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUProcessMemory) {
                if ($taskCounter.Name -match 'pid_([0-9]+)_') {
                    $taskGpuId = [int]$Matches[1]
                    if (-not $taskGpu.ContainsKey($taskGpuId)) { $taskGpu[$taskGpuId] = @(0L, 0L) }
                    $taskGpu[$taskGpuId][0] += [long]$taskCounter.DedicatedUsage; $taskGpu[$taskGpuId][1] += [long]$taskCounter.SharedUsage
                }
            }
        } catch {}
        $panelOwners = [Collections.Generic.Dictionary[int,long]]::new()
        foreach ($taskId in @($taskKnown)) {
            $taskNative = Get-PerformanceProcessSample -ProcessId $taskId -ExpectedCreationTime $taskCreated[$taskId]
            if ($null -eq $taskNative) { $skippedProcessSamples++; continue }
            if ($taskNative.name -match '^bongocat-(todo|mail|cyberwork)$') { $panelOwners[$taskId] = $taskCreated[$taskId].ToUniversalTime().ToFileTimeUtc() }
            $taskCpuNow = $taskNative.cpu_seconds; $taskCpuPercent = $null
            if ($taskCpu.ContainsKey($taskId) -and ($taskCpu[$taskId][2] -or -not $taskReady)) { $taskCpuPercent = 100 * ($taskCpuNow - $taskCpu[$taskId][0]) / ($taskElapsed - $taskCpu[$taskId][1]) / [Environment]::ProcessorCount }
            $taskCpu[$taskId] = @($taskCpuNow, $taskElapsed, $taskReady)
            $taskRows.Add([pscustomobject]@{seconds=$taskElapsed; sample=$taskReady; pid=$taskId; name=$taskNative.name; role=$(if ($taskId -eq $taskProcess.Id) { 'host-app' } elseif ($taskNative.name -eq 'msedgewebview2') { 'webview2' } elseif ($taskNative.name -eq 'conhost') { 'console' } else { 'native-plugin' }); private_commit=$taskNative.private_commit; private_working_set=$taskPrivate[$taskId]; working_set=$taskNative.working_set; cpu_percent=$taskCpuPercent; handles=$taskNative.handles; threads=$taskNative.threads; log_bytes=$(if ($taskId -eq $taskProcess.Id) { $taskLogBytes } else { $null }); gpu_dedicated=$(if ($taskGpu.ContainsKey($taskId)) { $taskGpu[$taskId][0] } else { $null }); gpu_shared=$(if ($taskGpu.ContainsKey($taskId)) { $taskGpu[$taskId][1] } else { $null })})
        }
        # Flush incrementally so an interrupted soak still has inspectable evidence.
        if ($taskRows.Count -gt 0) { $taskRows | Export-Csv -LiteralPath $taskCsv -NoTypeInformation -Append; $taskRows.Clear() }
        if ($Exercise -and $taskElapsed -ge 42 -and $taskElapsed - $lastClose -ge 12) {
            if ($panelOwners.Count -gt 0) { $closedWindows += [ProfileWindows]::Close($panelOwners,$profileRoot) }
            $lastClose = $taskElapsed
        }
        Start-Sleep -Milliseconds 1000
        $taskProcess.Refresh()
    }
    } catch {
        if (-not $taskProcess.HasExited) { $taskProcess.Kill(); $taskProcess.WaitForExit() }
        throw
    } finally {
        if ($KeepAwake) { [void][ProfileWindows]::SetThreadExecutionState([uint32]2147483648) }
    }
    $taskProcess.WaitForExit()
    if ($taskProcess.ExitCode -ne 0) { throw "Performance scenario failed; inspect $taskPrefix.stderr.txt and .stdout.txt" }
    if ($taskClock.Elapsed.TotalSeconds + 2 -lt $Seconds) { throw 'Profiling exited before the requested duration; this is incomplete evidence' }
    $cleanupDeadline = [DateTime]::UtcNow.AddSeconds(10)
    do {
        $remaining = @(Get-CimInstance Win32_Process -Property ProcessId,CreationDate)
        $residual = @($remaining | Where-Object { $taskKnown.Contains([int]$_.ProcessId) -and $taskCreated.ContainsKey([int]$_.ProcessId) -and $_.CreationDate -eq $taskCreated[[int]$_.ProcessId] } | ForEach-Object { [int]$_.ProcessId })
        if ($residual.Count -gt 0) { Start-Sleep -Milliseconds 250 }
    } while ($residual.Count -gt 0 -and [DateTime]::UtcNow -lt $cleanupDeadline)
    if ($residual.Count -gt 0) { throw "Isolated child processes remained after shutdown: $residual" }

$report=$null
if (Test-Path -LiteralPath $env:BONGOCAT_PERF_REPORT) {
    $report=Get-Content -Raw -Encoding UTF8 $env:BONGOCAT_PERF_REPORT | ConvertFrom-Json
    if ($report.frames_presented -lt 1) { throw 'No frames presented' }
} elseif (-not $ResourceOnly) { throw 'Frame report is required' }
if ($Exercise -and ($report.model_generation -lt 2 -or $closedWindows -lt 3)) { throw 'Model/panel exercise did not complete' }
[pscustomobject]@{started_utc=$taskStarted.ToString('o');completed_utc=[DateTime]::UtcNow.ToString('o');skipped_process_samples=$skippedProcessSamples; requested_seconds=$Seconds; warmup_seconds=$WarmupSeconds; seconds=$taskClock.Elapsed.TotalSeconds; closed_windows=$closedWindows; exit=$taskProcess.ExitCode; residual_processes=$residual; resource_only=[bool]$ResourceOnly; restart_plugins=[bool]$RestartPlugins; app_sha256=(Get-PerformanceFileHash -LiteralPath $appExecutable); frames=$report; resources=$taskCsv} | ConvertTo-Json -Depth 7 | Set-Content -Encoding utf8 (Join-Path $taskOutput 'summary.json')
Get-Content -Raw (Join-Path $taskOutput 'summary.json')
