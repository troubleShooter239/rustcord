#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [int]$RootPid,
    [ValidateRange(2, 3600)]
    [int]$Seconds = 30,
    [ValidateRange(1, 60)]
    [int]$Interval = 2
)

$ErrorActionPreference = 'Stop'
$root = Get-Process -Id $RootPid
$rootStart = $root.StartTime
$clock = [Diagnostics.Stopwatch]::StartNew()
$lastCpu = @{}
$cpuSeconds = 0.0
$samples = [Collections.Generic.List[object]]::new()

do {
    $root.Refresh()
    if ($root.HasExited -or $root.StartTime -ne $rootStart) {
        throw "Client PID $RootPid exited before sampling completed."
    }

    $snapshot = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId
    $children = $snapshot | Group-Object ParentProcessId -AsHashTable -AsString
    $ids = [Collections.Generic.List[int]]::new()
    $seen = [Collections.Generic.HashSet[int]]::new()
    $ids.Add($RootPid)
    $null = $seen.Add($RootPid)
    for ($i = 0; $i -lt $ids.Count; $i++) {
        foreach ($child in $children[[string]$ids[$i]]) {
            $childPid = [int]$child.ProcessId
            if ($seen.Add($childPid)) { $ids.Add($childPid) }
        }
    }

    $workingSet = 0L
    $privateBytes = 0L
    $processCount = 0
    foreach ($process in Get-Process -Id $ids -ErrorAction SilentlyContinue) {
        try {
            $key = "$($process.Id):$($process.StartTime.Ticks)"
            $cpu = $process.TotalProcessorTime.TotalSeconds
            $workingSet += $process.WorkingSet64
            $privateBytes += $process.PrivateMemorySize64
            $processCount++
        } catch {
            # A browser helper can exit between the two snapshots.
            continue
        }
        if ($lastCpu.ContainsKey($key)) {
            $cpuSeconds += $cpu - $lastCpu[$key]
        }
        $lastCpu[$key] = $cpu
    }
    $samples.Add([pscustomobject]@{
        Time = $clock.Elapsed.TotalSeconds
        Processes = $processCount
        WorkingSetMiB = $workingSet / 1MB
        PrivateMiB = $privateBytes / 1MB
    })

    $remaining = $Seconds - $clock.Elapsed.TotalSeconds
    if ($remaining -gt 0) { Start-Sleep -Seconds ([Math]::Min($Interval, $remaining)) }
} while ($remaining -gt 0)

if ($samples.Count -lt 2) { throw 'The measurement needs at least two samples.' }
$duration = $samples[-1].Time - $samples[0].Time
[pscustomobject]@{
    Client = $root.ProcessName
    RootPid = $RootPid
    Samples = $samples.Count
    ProcessesMax = ($samples.Processes | Measure-Object -Maximum).Maximum
    AvgWorkingSetMiB = [Math]::Round(($samples.WorkingSetMiB | Measure-Object -Average).Average, 1)
    AvgPrivateMiB = [Math]::Round(($samples.PrivateMiB | Measure-Object -Average).Average, 1)
    AvgCpuPercentOfOneCore = [Math]::Round(100 * $cpuSeconds / $duration, 2)
}
