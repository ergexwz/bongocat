param(
    [Parameter(Mandatory)][string]$TestExecutable,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [ValidateSet(0,3,10)][int]$PluginCount = 3,
    [ValidateSet(1,2)][int]$Protocol = 2,
    [string]$FixtureExecutable = '',
    [string]$PackageDirectory = '',
    [ValidateRange(0,300)][int]$WarmupSeconds = 30,
    [ValidateRange(1,28800)][int]$SampleSeconds = 120,
    [ValidateRange(1,3)][int]$Iterations = 3,
    [switch]$Exercise,
    [ValidateSet('mixed','actions','restarts')][string]$ExerciseMode = 'mixed',
    [ValidateRange(0,28800)][int]$IdleSeconds = 0,
    [ValidateRange(1,3600)][int]$RestartSeconds = 60
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'performance-processes.ps1')
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory)
[void][IO.Directory]::CreateDirectory($taskOutput)
$taskTemp = Join-Path $taskOutput 'isolated-temp'
[void][IO.Directory]::CreateDirectory($taskTemp)
$env:TEMP = $taskTemp; $env:TMP = $taskTemp
$env:WEBVIEW2_USER_DATA_FOLDER='.bongocat-profile-webview2'
$env:BONGOCAT_PERF_COUNT = "$PluginCount"; $env:BONGOCAT_PERF_PROTOCOL = "$Protocol"
$env:BONGOCAT_PERF_WARMUP = "$WarmupSeconds"; $env:BONGOCAT_PERF_SECONDS = "$SampleSeconds"
if ($Exercise) { $env:BONGOCAT_PERF_EXERCISE = '1' } else { Remove-Item Env:BONGOCAT_PERF_EXERCISE -ErrorAction SilentlyContinue }
$env:BONGOCAT_PERF_EXERCISE_MODE = $ExerciseMode
$env:BONGOCAT_PERF_IDLE_SECONDS = "$IdleSeconds"; $env:BONGOCAT_PERF_RESTART_SECONDS = "$RestartSeconds"
if ($IdleSeconds -gt $SampleSeconds) { throw 'IdleSeconds cannot exceed SampleSeconds' }
if ($FixtureExecutable) { $env:BONGOCAT_PERF_FIXTURE = [IO.Path]::GetFullPath($FixtureExecutable) } else { Remove-Item Env:BONGOCAT_PERF_FIXTURE -ErrorAction SilentlyContinue }
if ($PackageDirectory) { $env:BONGOCAT_PERF_PACKAGES = [IO.Path]::GetFullPath($PackageDirectory) } else { Remove-Item Env:BONGOCAT_PERF_PACKAGES -ErrorAction SilentlyContinue }
for ($taskIteration = 1; $taskIteration -le $Iterations; $taskIteration++) {
    $taskPrefix = Join-Path $taskOutput ("run-{0}-{1}" -f $taskIteration, [guid]::NewGuid().ToString('N'))
    $env:BONGOCAT_PERF_OUTPUT = "$taskPrefix.json"
    $taskProcess = Start-Process -FilePath ([IO.Path]::GetFullPath($TestExecutable)) -ArgumentList '--exact isolated_plugin_resource_scenario --ignored --nocapture' -WindowStyle Hidden -PassThru -RedirectStandardOutput "$taskPrefix.stdout.txt" -RedirectStandardError "$taskPrefix.stderr.txt"
    # Keep the native handle so Windows PowerShell retains the exit status.
    [void]$taskProcess.Handle
    $taskClock = [Diagnostics.Stopwatch]::StartNew()
    $taskKnown = [Collections.Generic.HashSet[int]]::new(); [void]$taskKnown.Add($taskProcess.Id)
    $taskCreated = @{}; $taskCpu = @{}
    $skippedProcessSamples = 0
    $taskRows = [Collections.Generic.List[object]]::new()
    $taskCsv = "$taskPrefix.csv"
    try {
    while (-not $taskProcess.HasExited) {
        # Follow descendants by identity, never by executable name or user data path.
        $taskAll = @(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,CreationDate)
        foreach ($taskId in @($taskKnown)) {
            $taskIdentity = $taskAll | Where-Object ProcessId -eq $taskId | Select-Object -First 1
            if ($null -eq $taskIdentity -or ($taskCreated.ContainsKey($taskId) -and $taskCreated[$taskId] -ne $taskIdentity.CreationDate)) { [void]$taskKnown.Remove($taskId); $taskCpu.Remove($taskId); $taskCreated.Remove($taskId) }
            elseif (-not $taskCreated.ContainsKey($taskId)) { $taskCreated[$taskId] = $taskIdentity.CreationDate }
        }
        do { $taskAdded = $false; foreach ($taskChild in $taskAll) { if ((Test-PerformanceDescendant -Child $taskChild -KnownProcesses $taskKnown -CreationTimes $taskCreated) -and -not $taskKnown.Contains([int]$taskChild.ProcessId)) { [void]$taskKnown.Add([int]$taskChild.ProcessId); $taskCreated[[int]$taskChild.ProcessId] = $taskChild.CreationDate; $taskAdded = $true } } } while ($taskAdded)
        $taskReady = Test-Path -LiteralPath $env:BONGOCAT_PERF_OUTPUT
        $taskElapsed = $taskClock.Elapsed.TotalSeconds
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
        foreach ($taskId in @($taskKnown)) {
            $taskNative = Get-PerformanceProcessSample -ProcessId $taskId -ExpectedCreationTime $taskCreated[$taskId]
            if ($null -eq $taskNative) { $skippedProcessSamples++; continue }
            $taskCpuNow = $taskNative.cpu_seconds; $taskCpuPercent = $null
            if ($taskCpu.ContainsKey($taskId) -and ($taskCpu[$taskId][2] -or -not $taskReady)) { $taskCpuPercent = 100 * ($taskCpuNow - $taskCpu[$taskId][0]) / ($taskElapsed - $taskCpu[$taskId][1]) / [Environment]::ProcessorCount }
            $taskCpu[$taskId] = @($taskCpuNow, $taskElapsed, $taskReady)
            $taskRows.Add([pscustomobject]@{seconds=$taskElapsed; sample=$taskReady; pid=$taskId; name=$taskNative.name; role=$(if ($taskId -eq $taskProcess.Id) { 'host-service' } elseif ($taskNative.name -eq 'msedgewebview2') { 'webview2' } elseif ($taskNative.name -eq 'conhost') { 'console' } else { 'native-plugin' }); private_commit=$taskNative.private_commit; private_working_set=$taskPrivate[$taskId]; working_set=$taskNative.working_set; cpu_percent=$taskCpuPercent; handles=$taskNative.handles; threads=$taskNative.threads; gpu_dedicated=$(if ($taskGpu.ContainsKey($taskId)) { $taskGpu[$taskId][0] } else { $null }); gpu_shared=$(if ($taskGpu.ContainsKey($taskId)) { $taskGpu[$taskId][1] } else { $null })})
        }
        # Flush incrementally so an interrupted soak still has inspectable evidence.
        if ($taskRows.Count -gt 0) { $taskRows | Export-Csv -LiteralPath $taskCsv -NoTypeInformation -Append; $taskRows.Clear() }
        Start-Sleep -Milliseconds 1000
        $taskProcess.Refresh()
    }
    } catch {
        if (-not $taskProcess.HasExited) { $taskProcess.Kill(); $taskProcess.WaitForExit() }
        throw
    }
    $taskProcess.WaitForExit()
    if ($taskProcess.ExitCode -ne 0) { throw "Performance scenario failed; inspect $taskPrefix.stderr.txt and .stdout.txt" }
    $taskReport = Get-Content -LiteralPath $env:BONGOCAT_PERF_OUTPUT -Raw | ConvertFrom-Json
    if (-not $taskReport.passed) { throw 'Scenario did not complete its shutdown checks' }
    [pscustomobject]@{ skipped_process_samples=$skippedProcessSamples; report=$env:BONGOCAT_PERF_OUTPUT; resources=$taskCsv; passed=$taskReport.passed } | ConvertTo-Json -Compress
}
