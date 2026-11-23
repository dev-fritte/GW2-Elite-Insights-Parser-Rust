<#
 Erzeugt die Per-Log-Tabelle fuer das Dokument zu Schritt 0.5 aus docs/phase0/results/*.tsv.
 Ausgabe auf stdout (Markdown).
#>
param([string]$ResultDir = (Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) '..\..\docs\phase0\results'), [ValidateSet("summary","perlog")][string]$Part = "perlog")
$inv = [Globalization.CultureInfo]::InvariantCulture
function N($s) { [double]::Parse(("$s" -replace ',', '.'), $inv) }
$tests = [ordered]@{
    'T0 Baseline (Mittel aus 3)' = @('baseline-1', 'baseline-2', 'baseline-3')
    'T0b Baseline erneut'         = @('base-recheck')
    'T1 TieredPGO aus'            = @('cfg-tieredpgo-off')
    'T2 TieredCompilation aus'    = @('cfg-tiered-off')
    'T3 GC nicht-konkurrent'      = @('cfg-gcconc-off')
    'T4 Gen0-Budget 256 MB'       = @('cfg-gen0-256m')
    'T5 PGO aus + GC nicht-konkurrent + Gen0 256 MB' = @('cfg-combo')
    'T6 Code: Buff-Intensity-Simulation'  = @('code-buffsim')
    'T7 Code: Binaersuche Zeitfilter'     = @('code-slice')
    'T8 ReadyToRun'               = @('r2r-default')
    'T9 ReadyToRun + TieredPGO aus' = @('r2r-tieredpgo-off')
    'T10 Multithread-Parse (CLI-Standard)' = @('baseline-threads')
}
$data = @{}
foreach ($t in $tests.Keys) {
    $acc = @{}
    foreach ($l in $tests[$t]) {
        foreach ($r in (Import-Csv -Delimiter "`t" -LiteralPath (Join-Path $ResultDir "$l.tsv") | Where-Object Status -eq 'OK')) {
            $k = $r.TargetId + '|' + $r.File
            if (-not $acc.ContainsKey($k)) { $acc[$k] = @() }
            $acc[$k] += $r
        }
    }
    $data[$t] = $acc
}
$manifest = Import-Csv -Delimiter "`t" -LiteralPath (Join-Path $ResultDir '..\corpus-manifest.tsv')
function Avg($rows, $n) { ($rows | ForEach-Object { N $_.$n } | Measure-Object -Average).Average }
if ($Part -eq 'perlog') {
'| Ordner | Log | Bytes | ' + (($tests.Keys | ForEach-Object { ($_ -split ' ')[0] }) -join ' | ') + ' |'
'| --- | --- | ---: | ' + (($tests.Keys | ForEach-Object { '---:' }) -join ' | ') + ' |'
foreach ($m in $manifest) {
    $k = $m.TargetId + '|' + $m.File
    $cells = foreach ($t in $tests.Keys) { if ($data[$t].ContainsKey($k)) { '{0:N0}' -f (Avg $data[$t][$k] 'Wall_ms') } else { '-' } }
    '| {0} | {1} | {2:N0} | {3} |' -f $m.Folder, $m.File, [double]$m.Bytes, ($cells -join ' | ')
}
}
if ($Part -eq "summary") {
    function Med($v) { $s = @($v | Sort-Object); $s[[int][math]::Floor(($s.Count - 1) / 2)] }
    function P90($v) { $s = @($v | Sort-Object); $s[[int][math]::Floor(($s.Count - 1) * 0.9)] }
    $baseWall = $null
    "| Test | Logs | Wall Summe s | Wall Median ms | Wall P90 ms | Total Summe s | Parse Median ms | Build Median ms | Peak Median MB | Alloc Summe GB | Wall vs T0b |"
    "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"
    foreach ($t in $tests.Keys) {
        $keys = @($data[$t].Keys)
        $col = { param($n) $keys | ForEach-Object { Avg $data[$t][$_] $n } }
        $wall = & $col "Wall_ms"; $tot = & $col "Total_ms"
        $ws = ($wall | Measure-Object -Sum).Sum
        if ($t -like "T0b*") { $baseWall = $ws }
        $rel = if ($baseWall) { "{0:+0.0;-0.0}%" -f (($ws / $baseWall - 1) * 100) } else { "" }
        "| {0} | {1} | {2:N1} | {3:N0} | {4:N0} | {5:N1} | {6:N0} | {7:N0} | {8:N0} | {9:N1} | {10} |" -f $t, $keys.Count, ($ws / 1000), (Med $wall), (P90 $wall), ((($tot | Measure-Object -Sum).Sum) / 1000), (Med (& $col "Parse_ms")), (Med (& $col "Build_ms")), (Med (& $col "PeakWS_MB")), ((((& $col "Alloc_MB") | Measure-Object -Sum).Sum) / 1024), $rel
    }
}
