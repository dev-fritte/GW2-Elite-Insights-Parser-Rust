<#
 Phase 1.0: laeuft GW2EIParserReference --oracle ueber das Korpus-Manifest.
 Schreibt pro Log einen vollstaendigen Dump nach -DumpDir (lokal, enthaelt Spielernamen) und
 eine TSV mit Zaehlern und Hashes nach -Out (diese darf ins Repo).
#>
param(
    [Parameter(Mandatory)][string]$CorpusRoot,
    [Parameter(Mandatory)][string]$Out,
    [string]$DumpDir = '',
    [string]$Exe = '',
    [string]$Manifest = ''
)
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
if (-not $Exe) { $Exe = Join-Path $here '..\..\GW2EI.bin\Release.ref\GW2EIParserReference.exe' }
if (-not $Manifest) { $Manifest = Join-Path $here '..\..\docs\phase0\corpus-manifest.tsv' }
$Exe = [System.IO.Path]::GetFullPath($Exe)
if ($DumpDir) { New-Item -ItemType Directory -Force -Path $DumpDir | Out-Null }
$rows = foreach ($r in (Import-Csv -Delimiter "`t" -LiteralPath $Manifest)) {
    $log = Join-Path (Join-Path $CorpusRoot $r.Folder) $r.File
    $dump = if ($DumpDir) { Join-Path $DumpDir ($r.TargetId + '_' + $r.File + '.json') } else { '-' }
    $line = (& $Exe --log $log --oracle $dump 2>&1 | Where-Object { "$_" -like 'ORACLE *' } | Select-Object -Last 1)
    if (-not $line) { [pscustomobject]@{ Folder = $r.Folder; TargetId = $r.TargetId; File = $r.File; Status = 'FAIL' }; continue }
    $j = ("$line" -replace '^ORACLE ', '') | ConvertFrom-Json
    [pscustomobject]@{
        Folder = $r.Folder; TargetId = $r.TargetId; File = $r.File; Status = 'OK'
        BuildString = $j.header.build_string; Revision = $j.header.revision; TriggerId = $j.header.trigger_id
        Agents = $j.agents.count; AgentsHash = $j.agents.hash
        SkillsRaw = $j.skills.count_raw; SkillsUnique = $j.skills.unique; SkillsHash = $j.skills.hash
        Events = $j.events.count; TrailingBytes = $j.events.trailing_bytes; EventsHash = $j.events.hash
        KeptCount = $j.kept.count; KeptHash = $j.kept.hash; KeptTriggerId = $j.kept.trigger_id; MapId = $j.kept.map_id
        LogStartOffset = $j.kept.log_start_offset; LogEndTime = $j.kept.log_end_time; Gw2Build = $j.kept.gw2_build
        KeptError = $j.kept_error
    }
}
$lines = @($rows | ConvertTo-Csv -Delimiter "`t" -NoTypeInformation | ForEach-Object { $_ -replace '"', '' })
Set-Content -LiteralPath ([System.IO.Path]::GetFullPath($Out)) -Value $lines -Encoding utf8
$ok = @($rows | Where-Object Status -eq 'OK').Count
"{0}/{1} OK -> {2}" -f $ok, @($rows).Count, $Out
