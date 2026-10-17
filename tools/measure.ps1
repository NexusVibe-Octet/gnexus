# Measures the RAM and CPU of a program with all its child processes (its WebView windows
# included), sampled every second.
#   powershell -ExecutionPolicy Bypass -File tools\measure.ps1 -Names "G Nexus" -Seconds 60
#   ... -Names lghub,lghub_agent,lghub_updater,lghub_system_tray     (G HUB)
#   ... -Names LCore                                                  (Logitech Gaming Software)
param(
    [Parameter(Mandatory)] [string[]] $Names,
    [int] $Seconds = 60
)

function Get-Tree {
    $all = Get-CimInstance Win32_Process
    $ids = @($all | Where-Object { $Names -contains [IO.Path]::GetFileNameWithoutExtension($_.Name) } | ForEach-Object { $_.ProcessId })
    $queue = [System.Collections.Generic.Queue[int]]::new()
    $ids | ForEach-Object { $queue.Enqueue($_) }
    $seen = @{}
    while ($queue.Count) {
        $id = $queue.Dequeue()
        if ($seen[$id]) { continue }
        $seen[$id] = $true
        $all | Where-Object { $_.ParentProcessId -eq $id } | ForEach-Object { $queue.Enqueue($_.ProcessId) }
    }
    @($seen.Keys)
}

$Names = @($Names | ForEach-Object { $_ -split "," } | ForEach-Object { $_.Trim() })
$cores = [Environment]::ProcessorCount
$ids = Get-Tree
if (-not $ids.Count) { Write-Host "Aucun processus trouvé pour : $($Names -join ', ')"; exit 1 }

$cpuStart = @{}
foreach ($id in $ids) { try { $cpuStart[$id] = (Get-Process -Id $id -ErrorAction Stop).TotalProcessorTime.TotalMilliseconds } catch {} }
$clock = [Diagnostics.Stopwatch]::StartNew()
$ws = @(); $priv = @()
for ($i = 0; $i -lt $Seconds; $i++) {
    Start-Sleep -Seconds 1
    $procs = $ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }
    $ws += ($procs | Measure-Object WorkingSet64 -Sum).Sum
    $priv += ($procs | Measure-Object PrivateMemorySize64 -Sum).Sum
}
$cpuMs = 0
foreach ($id in $ids) { try { $cpuMs += (Get-Process -Id $id -ErrorAction Stop).TotalProcessorTime.TotalMilliseconds - $cpuStart[$id] } catch {} }
$elapsed = $clock.Elapsed.TotalMilliseconds

$procs = $ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }
Write-Host ("Processus : {0}  ({1})" -f $procs.Count, (($procs | Group-Object ProcessName | ForEach-Object { "$($_.Name) x$($_.Count)" }) -join ', '))
Write-Host ("RAM (working set) : moyenne {0:N0} Mo, pic {1:N0} Mo" -f (($ws | Measure-Object -Average).Average / 1MB), (($ws | Measure-Object -Maximum).Maximum / 1MB))
Write-Host ("RAM privée        : moyenne {0:N0} Mo" -f (($priv | Measure-Object -Average).Average / 1MB))
Write-Host ("CPU               : {0:N2} % de la machine ({1} cœurs) sur {2} s" -f ($cpuMs / $elapsed / $cores * 100), $cores, $Seconds)
