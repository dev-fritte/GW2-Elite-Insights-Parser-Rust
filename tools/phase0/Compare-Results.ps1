<#
 Vergleicht zwei Ergebnis-TSVs (Invoke-Corpus.ps1) Log fuer Log.
 -Parity : prueft JsonSha256 (Paritaet), gibt Abweichungen aus.
 Immer: Summen fuer Total_ms, Parse_ms, Build_ms, Json_ms, Wall_ms, Alloc_MB und PeakWS_MB (Median) beider Laeufe.
#>
param(
    [Parameter(Mandatory)][string]$PathA,
    [Parameter(Mandatory)][string]$PathB,
    [switch]$Parity
)
$ra = Import-Csv -Delimiter "`t" -LiteralPath $PathA | Where-Object Status -eq 'OK'
$rb = Import-Csv -Delimiter "`t" -LiteralPath $PathB | Where-Object Status -eq 'OK'
$key = { param($r) $r.TargetId + '|' + $r.File }
$bm = @{}; foreach ($r in $rb) { $bm[(& $key $r)] = $r }
$diff = 0; $same = 0; $missing = 0
foreach ($r in $ra) {
    $o = $bm[(& $key $r)]
    if (-not $o) { $missing++; continue }
    if ($r.JsonSha256 -eq $o.JsonSha256) { $same++ } else { $diff++; if ($Parity) { "ABWEICHUNG: {0} {1}" -f $r.Folder, $r.File } }
}
"Gleiche JSON-Hashes: $same, abweichend: $diff, fehlend in B: $missing"
function Summ($rows, $n) {
    $s = ($rows | ForEach-Object { [double]::Parse(($_.$n -replace "," , "."), [Globalization.CultureInfo]::InvariantCulture) } | Measure-Object -Sum).Sum
    $med = ($rows | ForEach-Object { [double]::Parse(($_.$n -replace "," , "."), [Globalization.CultureInfo]::InvariantCulture) } | Sort-Object)[[int][math]::Floor($rows.Count / 2)]
    "{0,-12} Summe {1,10:N1}  Median {2,8:N1}" -f $n, $s, $med
}
foreach ($set in @(@("A", $ra), @("B", $rb))) {
    "--- $($set[0]) ($($set[1].Count) Logs)"
    foreach ($n in 'Wall_ms', 'Total_ms', 'Parse_ms', 'Build_ms', 'Json_ms', 'Alloc_MB', 'PeakWS_MB') { Summ $set[1] $n }
}
