param(
    [Parameter(Mandatory)][int]$RootProcessId,
    [ValidateRange(1, 600)][int]$Seconds = 600,
    [Parameter(Mandatory)][string]$OutputCsv
)
$ErrorActionPreference = 'Stop'
# Read only process/counter metadata. Do not attach a debugger or open a game.
$bongoClock = [Diagnostics.Stopwatch]::StartNew()
$bongoRows = [Collections.Generic.List[object]]::new()
$bongoKnown = [Collections.Generic.HashSet[int]]::new()
[void]$bongoKnown.Add($RootProcessId)
$bongoCreationTimes = @{}
$bongoCpuPrevious = @{}
$bongoGpuAvailable = $true
while ($bongoClock.Elapsed.TotalSeconds -lt $Seconds) {
    $bongoProcesses = @(Get-CimInstance Win32_Process -Property ProcessId,ParentProcessId,Name,CreationDate)
    foreach ($bongoProcess in $bongoProcesses) {
        $bongoProcessId = [int]$bongoProcess.ProcessId
        if (-not $bongoKnown.Contains($bongoProcessId)) { continue }
        if ($bongoCreationTimes.ContainsKey($bongoProcessId) -and
            $bongoCreationTimes[$bongoProcessId] -ne $bongoProcess.CreationDate) {
            [void]$bongoKnown.Remove($bongoProcessId)
            $bongoCpuPrevious.Remove($bongoProcessId)
        } else {
            $bongoCreationTimes[$bongoProcessId] = $bongoProcess.CreationDate
        }
    }
    do {
        $bongoAdded = $false
        foreach ($bongoProcess in $bongoProcesses) {
            if ($bongoKnown.Contains([int]$bongoProcess.ParentProcessId)) {
                if ($bongoKnown.Add([int]$bongoProcess.ProcessId)) {
                    $bongoCreationTimes[[int]$bongoProcess.ProcessId] = $bongoProcess.CreationDate
                    $bongoAdded = $true
                }
            }
        }
    } while ($bongoAdded)
    $bongoPrivateWorkingSets = @{}
    foreach ($bongoCounter in @(Get-CimInstance Win32_PerfFormattedData_PerfProc_Process)) {
        if ($bongoKnown.Contains([int]$bongoCounter.IDProcess)) {
            $bongoPrivateWorkingSets[[int]$bongoCounter.IDProcess] = [long]$bongoCounter.WorkingSetPrivate
        }
    }
    $bongoGpu = @{}
    if ($bongoGpuAvailable) {
        try {
            foreach ($bongoCounter in @(Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUProcessMemory)) {
                if ($bongoCounter.Name -match '^pid_(\d+)_') {
                    $bongoCounterId = [int]$Matches[1]
                    if ($bongoKnown.Contains($bongoCounterId)) {
                        if (-not $bongoGpu.ContainsKey($bongoCounterId)) { $bongoGpu[$bongoCounterId] = @(0L, 0L) }
                        $bongoGpu[$bongoCounterId][0] += [long]$bongoCounter.DedicatedUsage
                        $bongoGpu[$bongoCounterId][1] += [long]$bongoCounter.SharedUsage
                    }
                }
            }
        } catch { $bongoGpuAvailable = $false }
    }
    $bongoElapsed = $bongoClock.Elapsed.TotalSeconds
    $bongoAlive = 0
    foreach ($bongoProcess in $bongoProcesses) {
        $bongoProcessId = [int]$bongoProcess.ProcessId
        if (-not $bongoKnown.Contains($bongoProcessId)) { continue }
        $bongoNative = Get-Process -Id $bongoProcessId -ErrorAction SilentlyContinue
        if ($null -eq $bongoNative) { continue }
        $bongoAlive++
        $bongoCpu = $bongoNative.TotalProcessorTime.TotalSeconds
        $bongoCpuPercent = $null
        if ($bongoCpuPrevious.ContainsKey($bongoProcessId)) {
            $bongoPrior = $bongoCpuPrevious[$bongoProcessId]
            $bongoCpuPercent = 100 * ($bongoCpu - $bongoPrior[0]) / ($bongoElapsed - $bongoPrior[1]) / [Environment]::ProcessorCount
        }
        $bongoCpuPrevious[$bongoProcessId] = @($bongoCpu, $bongoElapsed)
        $bongoRows.Add([pscustomobject]@{
            seconds = [Math]::Round($bongoElapsed, 3)
            process_id = $bongoProcessId
            parent_id = $bongoProcess.ParentProcessId
            name = $bongoProcess.Name
            private_commit_bytes = $bongoNative.PrivateMemorySize64
            private_working_set_bytes = $bongoPrivateWorkingSets[$bongoProcessId]
            working_set_bytes = $bongoNative.WorkingSet64
            cpu_percent = $bongoCpuPercent
            handles = $bongoNative.HandleCount
            threads = $bongoNative.Threads.Count
            gpu_dedicated_bytes = $(if ($bongoGpu.ContainsKey($bongoProcessId)) { $bongoGpu[$bongoProcessId][0] } else { $null })
            gpu_shared_bytes = $(if ($bongoGpu.ContainsKey($bongoProcessId)) { $bongoGpu[$bongoProcessId][1] } else { $null })
        })
    }
    if ($bongoAlive -eq 0) { break }
    # Never follow a stale PID into a subsequently launched unrelated process.
    foreach ($bongoKnownId in @($bongoKnown)) {
        if (-not ($bongoProcesses.ProcessId -contains $bongoKnownId)) { [void]$bongoKnown.Remove($bongoKnownId) }
    }
    Start-Sleep -Milliseconds 1000
}
$bongoParent = Split-Path -Parent ([IO.Path]::GetFullPath($OutputCsv))
[void][IO.Directory]::CreateDirectory($bongoParent)
$bongoRows | Export-Csv -LiteralPath $OutputCsv -NoTypeInformation -Encoding utf8
[pscustomobject]@{ seconds = $bongoClock.Elapsed.TotalSeconds; samples = $bongoRows.Count; gpu_counter_available = $bongoGpuAvailable; output = $OutputCsv } | ConvertTo-Json -Compress
