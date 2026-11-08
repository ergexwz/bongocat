# Hashing must also work when a caller inherits another PowerShell's module path.
function Get-PerformanceFileHash {
    param([string]$LiteralPath)
    $stream = $null; $hasher = $null
    try {
        $stream = [IO.File]::OpenRead($LiteralPath)
        $hasher = [Security.Cryptography.SHA256]::Create()
        return -join ($hasher.ComputeHash($stream) | ForEach-Object { $_.ToString('x2') })
    } finally {
        if ($null -ne $stream) { $stream.Dispose() }
        if ($null -ne $hasher) { $hasher.Dispose() }
    }
}

# A process can exit between enumeration and any property read. Copy a complete,
# identity-checked sample, then release our observer handle; never retain Process.
function Get-PerformanceProcessSample {
    param([int]$ProcessId, [datetime]$ExpectedCreationTime)
    $observed = $null
    try {
        $observed = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
        if ($null -eq $observed -or $observed.HasExited) { return $null }
        $started = $observed.StartTime
        if ($null -eq $started -or [Math]::Abs(($started.ToUniversalTime() - $ExpectedCreationTime.ToUniversalTime()).TotalMilliseconds) -gt 1) { return $null }
        $sample = [pscustomobject]@{
            name = $observed.ProcessName
            cpu_seconds = $observed.TotalProcessorTime.TotalSeconds
            private_commit = $observed.PrivateMemorySize64
            working_set = $observed.WorkingSet64
            handles = $observed.HandleCount
            threads = $observed.Threads.Count
        }
        if ($observed.HasExited) { return $null }
        return $sample
    } catch {
        # An exited or temporarily unreadable child is not an application fault.
        # The caller counts unavailable samples and rechecks identity next time.
        return $null
    } finally {
        if ($null -ne $observed) { $observed.Dispose() }
    }
}

# ParentProcessId survives the parent's exit and can point at a reused PID.
# A child older than its currently tracked parent cannot belong to this run.
function Test-PerformanceDescendant {
    param($Child, [Collections.Generic.HashSet[int]]$KnownProcesses, [hashtable]$CreationTimes)
    $parentId = [int]$Child.ParentProcessId
    if (-not $KnownProcesses.Contains($parentId) -or -not $CreationTimes.ContainsKey($parentId) -or $null -eq $Child.CreationDate -or $null -eq $CreationTimes[$parentId]) { return $false }
    return $Child.CreationDate.ToUniversalTime() -ge $CreationTimes[$parentId].ToUniversalTime()
}
