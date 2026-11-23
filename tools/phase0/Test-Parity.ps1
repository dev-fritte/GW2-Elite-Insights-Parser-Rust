<#
 Prueft ein Ergebnis-TSV (Invoke-Corpus.ps1) gegen docs/phase0/reference-hashes.tsv.
 Ausgabe: "<Label>: x/y Logs paritaetisch" und die Liste der Abweichungen.
#>
param(
    [Parameter(Mandatory)][string]$Label,
    [string]$ResultDir = 'D:\_Projekte\GW2EI-Phase0\results',
    [string]$Reference = ''
)
if (-not $Reference) { $Reference = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) '..\..\docs\phase0\reference-hashes.tsv' }
$ref = @{}
foreach ($r in (Import-Csv -Delimiter "`t" -LiteralPath $Reference)) { $ref[$r.TargetId + '|' + $r.File] = $r.JsonSha256 }
$rows = Import-Csv -Delimiter "`t" -LiteralPath (Join-Path $ResultDir "$Label.tsv")
$ok = 0; $bad = @()
foreach ($r in $rows) {
    if ($r.Status -eq 'OK' -and $ref[$r.TargetId + '|' + $r.File] -eq $r.JsonSha256) { $ok++ } else { $bad += $r.Folder + ' ' + $r.File }
}
"{0}: {1}/{2} Logs paritaetisch" -f $Label, $ok, $ref.Count
$bad | ForEach-Object { "  ABWEICHUNG: $_" }
