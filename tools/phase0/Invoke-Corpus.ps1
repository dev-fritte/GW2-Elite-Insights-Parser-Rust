<#
 Fuehrt GW2EIParserReference (ein Prozess pro Log) ueber das Manifest aus und sammelt die RESULT-Zeilen.
 Beispiel (Referenz):  Invoke-Corpus.ps1 -CorpusRoot <pfad> -Label ref-run1 -JsonDir D:\GW2EI-Phase0\ref1
 Beispiel (Messung):   Invoke-Corpus.ps1 -CorpusRoot <pfad> -Label baseline -Exe <exe> -Runs 1
 Ausgabe: <ResultDir>\<Label>.tsv (eine Zeile pro Log)
#>
param(
    [Parameter(Mandatory)][string]$CorpusRoot,
    [Parameter(Mandatory)][string]$Label,
    [string]$Exe = (Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) '..\..\GW2EI.bin\Release.ref\GW2EIParserReference.exe'),
    [string]$Manifest = (Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) '..\..\docs\phase0\corpus-manifest.tsv'),
    [string]$ResultDir = 'D:\_Projekte\GW2EI-Phase0\results',
    [string]$JsonDir = '',
    [int]$Runs = 1,
    [switch]$Threads,
    [switch]$NoJson,
    [hashtable]$Env = @{},
    [string]$Filter = ''
)
[Threading.Thread]::CurrentThread.CurrentCulture = [Globalization.CultureInfo]::InvariantCulture
$Exe = [System.IO.Path]::GetFullPath($Exe)
New-Item -ItemType Directory -Force -Path $ResultDir | Out-Null
if ($JsonDir) { New-Item -ItemType Directory -Force -Path $JsonDir | Out-Null }
foreach ($k in $Env.Keys) { [Environment]::SetEnvironmentVariable($k, [string]$Env[$k], 'Process') }
$rows = Import-Csv -Delimiter "`t" -LiteralPath $Manifest
if ($Filter) { $rows = $rows | Where-Object { $_.Folder -match $Filter } }
$out = New-Object System.Collections.Generic.List[object]
foreach ($r in $rows) {
    $log = Join-Path (Join-Path $CorpusRoot $r.Folder) $r.File
    $a = @('--log', $log, '--runs', $Runs)
    if ($Threads) { $a += '--threads' }
    if ($NoJson) { $a += '--no-json' }
    if ($JsonDir) { $a += @('--out', (Join-Path $JsonDir ($r.TargetId + '_' + $r.File + '.json'))) }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $lines = & $Exe @a 2>&1
    $wall = $sw.Elapsed.TotalMilliseconds
    $res = ($lines | Where-Object { "$_" -like 'RESULT *' } | Select-Object -Last 1)
    if (-not $res) {
        $out.Add([pscustomobject]@{ Folder = $r.Folder; TargetId = $r.TargetId; File = $r.File; Bytes = $r.Bytes; Status = 'FAIL'; Wall_ms = [math]::Round($wall, 1); Error = (($lines | Select-Object -Last 2) -join ' | ') })
        continue
    }
    $j = ($res -replace '^RESULT ', '') | ConvertFrom-Json
    $last = $j.runs[-1]
    $first = $j.runs[0]
    $out.Add([pscustomobject]@{
        Folder = $r.Folder; TargetId = $r.TargetId; File = $r.File; Bytes = $r.Bytes; Status = 'OK'
        Wall_ms = [math]::Round($wall, 1)
        Parse_ms = $first.parse_ms; Build_ms = $first.build_ms; Json_ms = $first.json_ms; Total_ms = $first.total_ms
        WarmTotal_ms = $last.total_ms; Alloc_MB = $first.allocated_mb
        PeakWS_MB = $j.peak_working_set_mb; Gen0 = $j.gc_gen0; Gen1 = $j.gc_gen1; Gen2 = $j.gc_gen2
        ApiLoad_ms = $j.api_load_ms; JsonBytes = $j.json_bytes; JsonSha256 = $j.json_sha256; Error = ''
    })
}
$path = Join-Path $ResultDir "$Label.tsv"
$out | Export-Csv -Delimiter "`t" -NoTypeInformation -LiteralPath $path -Encoding utf8
$ok = @($out | Where-Object Status -eq 'OK')
"{0}: {1}/{2} OK, Summe Total_ms {3:N0}, Datei {4}" -f $Label, $ok.Count, $out.Count, ($ok | ForEach-Object { [double]$_.Total_ms } | Measure-Object -Sum).Sum, $path
