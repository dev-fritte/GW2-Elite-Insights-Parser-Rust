<#
 Misst den Rust-Core (ei.exe) pro Log: Wall-Zeit und Spitzen-Arbeitsspeicher.
 Der Speicher wird alle 2 ms abgefragt, der Wert ist daher eine untere Schaetzung des echten Spitzenwerts.
 Ausgabe: TSV mit einer Zeile pro Log nach -Out.
#>
param(
    [Parameter(Mandatory)][string]$CorpusRoot,
    [Parameter(Mandatory)][string]$Exe,
    [Parameter(Mandatory)][string]$Content,
    [Parameter(Mandatory)][string]$Manifest,
    [Parameter(Mandatory)][string]$Out
)
[Threading.Thread]::CurrentThread.CurrentCulture = [Globalization.CultureInfo]::InvariantCulture
$rows = foreach ($r in (Import-Csv -Delimiter "`t" -LiteralPath $Manifest)) {
    $log = Join-Path (Join-Path $CorpusRoot $r.Folder) $r.File
    $tmp = [System.IO.Path]::GetTempFileName()
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $Exe -ArgumentList @("`"$log`"", '--content', "`"$Content`"", '--out', "`"$tmp`"") -PassThru -NoNewWindow
    $null = $p.Handle  # keeps the exit code readable after the process ended
    $peak = 0
    while (-not $p.HasExited) {
        try { $p.Refresh(); if ($p.PeakWorkingSet64 -gt $peak) { $peak = $p.PeakWorkingSet64 } } catch { }
        Start-Sleep -Milliseconds 2
    }
    $p.WaitForExit()
    $wall = $sw.Elapsed.TotalMilliseconds
    Remove-Item -LiteralPath $tmp -ErrorAction SilentlyContinue
    [pscustomobject]@{
        Folder = $r.Folder; TargetId = $r.TargetId; File = $r.File; Bytes = $r.Bytes
        ExitCode = $p.ExitCode; Wall_ms = [math]::Round($wall, 1); PeakWS_MB = [math]::Round($peak / 1MB, 1)
    }
}
$rows | Export-Csv -Delimiter "`t" -NoTypeInformation -LiteralPath $Out -Encoding utf8
$ok = @($rows | Where-Object ExitCode -eq 0)
$w = $ok | ForEach-Object { $_.Wall_ms } | Sort-Object
$m = $ok | ForEach-Object { $_.PeakWS_MB } | Sort-Object
"{0}/{1} OK, Wall: Summe {2:N0} ms, Median {3:N0} ms, P90 {4:N0} ms, Max {5:N0} ms; PeakWS: Median {6:N1} MB, Max {7:N1} MB" -f `
    $ok.Count, @($rows).Count, ($w | Measure-Object -Sum).Sum, $w[[int][math]::Floor(($w.Count - 1) / 2)], $w[[int][math]::Floor(($w.Count - 1) * 0.9)], $w[-1], $m[[int][math]::Floor(($m.Count - 1) / 2)], $m[-1]
