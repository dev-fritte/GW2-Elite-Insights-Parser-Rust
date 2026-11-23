<#
 Erzeugt das kuratierte Korpus-Manifest (Phase 0.1).
 Pro Boss-Ordner: das Log mit der mittleren Dateigroesse; bei Ordnern mit >= 20 Logs zusaetzlich das groesste.
 Die Logs selbst bleiben ausserhalb des Repos (enthalten Spieler- und Account-Namen).
 Das Manifest enthaelt nur Ordnername, Dateiname (Zeitstempel), Groesse und SHA256.
#>
param(
    [Parameter(Mandatory)][string]$CorpusRoot,
    [string]$Out = ''
)
if (-not $Out) { $Out = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) '..\..\docs\phase0\corpus-manifest.tsv' }
$Out = [System.IO.Path]::GetFullPath($Out)
$rows = foreach ($dir in Get-ChildItem -LiteralPath $CorpusRoot -Directory | Sort-Object Name) {
    $files = Get-ChildItem -LiteralPath $dir.FullName -File |
        Where-Object { $_.Extension -in '.evtc', '.zevtc' } | Sort-Object Length, Name
    if ($files.Count -eq 0) { continue }
    $pick = @($files[[int][math]::Floor(($files.Count - 1) / 2)])
    if ($files.Count -ge 20) { $pick += $files[-1] }
    $id = if ($dir.Name -match '\((\d+)\)$') { $Matches[1] } else { '' }
    foreach ($f in ($pick | Sort-Object Name -Unique)) {
        [pscustomobject]@{
            Folder = $dir.Name; TargetId = $id; File = $f.Name; Bytes = $f.Length
            FolderLogCount = $files.Count
            Sha256 = (Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256).Hash
        }
    }
}
$lines = @($rows | ConvertTo-Csv -Delimiter "`t" -NoTypeInformation | ForEach-Object { $_ -replace '"', '' })
Set-Content -LiteralPath $Out -Value $lines -Encoding utf8
"{0} Logs, {1:N1} MB" -f $rows.Count, (($rows | Measure-Object Bytes -Sum).Sum / 1MB)
