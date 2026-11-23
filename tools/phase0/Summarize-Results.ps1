<#
 Fasst mehrere Ergebnis-TSVs (Wiederholungen derselben Konfiguration) zu einer Markdown-Zusammenfassung zusammen.
 Pro Log wird der Mittelwert ueber die Wiederholungen gebildet.
#>
param(
    [Parameter(Mandatory)][string[]]$Labels,
    [string]$ResultDir = 'D:\_Projekte\GW2EI-Phase0\results',
    [int]$Top = 0
)
$Labels = @($Labels | ForEach-Object { $_ -split "," })
$inv = [Globalization.CultureInfo]::InvariantCulture
function N($s) { [double]::Parse(("$s" -replace ',', '.'), $inv) }
$all = @{}
foreach ($l in $Labels) {
    foreach ($r in (Import-Csv -Delimiter "`t" -LiteralPath (Join-Path $ResultDir "$l.tsv") | Where-Object Status -eq 'OK')) {
        $k = $r.TargetId + '|' + $r.File
        if (-not $all.ContainsKey($k)) { $all[$k] = @() }
        $all[$k] += $r
    }
}
$rows = foreach ($k in $all.Keys) {
    $g = $all[$k]
    $f = {param($n) ($g | ForEach-Object { N $_.$n } | Measure-Object -Average).Average}
    [pscustomobject]@{
        Folder = $g[0].Folder; File = $g[0].File; Bytes = N $g[0].Bytes
        Wall = & $f 'Wall_ms'; Total = & $f 'Total_ms'; Parse = & $f 'Parse_ms'; Build = & $f 'Build_ms'; Json = & $f 'Json_ms'
        Alloc = & $f 'Alloc_MB'; Peak = & $f 'PeakWS_MB'; Api = & $f 'ApiLoad_ms'
    }
}
function Stat($n) {
    $v = @($rows | ForEach-Object { $_.$n } | Sort-Object)
    $c = $v.Count
    '| {0} | {1:N0} | {2:N0} | {3:N0} | {4:N0} | {5:N0} |' -f $n, ($v | Measure-Object -Sum).Sum, $v[[int][math]::Floor(($c - 1) / 2)], $v[[int][math]::Floor(($c - 1) * 0.9)], $v[-1], ($v | Measure-Object -Average).Average
}
"Konfiguration: $($Labels -join ', ') ($($rows.Count) Logs, Mittel ueber $($Labels.Count) Lauf/Laeufe)"
''
'| Messgroesse | Summe | Median | P90 | Max | Mittel |'
'| --- | ---: | ---: | ---: | ---: | ---: |'
foreach ($n in 'Wall', 'Total', 'Parse', 'Build', 'Json', 'Alloc', 'Peak', 'Api') { Stat $n }
if ($Top -gt 0) {
    ''
    '| Log | Datei | Bytes | Total ms | Parse ms | Build ms | Json ms | Peak MB |'
    '| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |'
    $rows | Sort-Object Total -Descending | Select-Object -First $Top | ForEach-Object {
        '| {0} | {1} | {2:N0} | {3:N0} | {4:N0} | {5:N0} | {6:N0} | {7:N0} |' -f $_.Folder, $_.File, $_.Bytes, $_.Total, $_.Parse, $_.Build, $_.Json, $_.Peak
    }
}
