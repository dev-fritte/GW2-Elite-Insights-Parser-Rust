# Phase 0.1: Kuratierter Test-Korpus

Die Rohlogs (2.531 `.zevtc`, 3,5 GB, 60 Boss-Ordner) liegen lokal und gehören **nicht** ins Repo, weil sie Spieler- und Account-Namen enthalten.
Ins Repo kommt nur das Manifest [`corpus-manifest.tsv`](corpus-manifest.tsv) (Ordner, Dateiname, Größe, SHA256).

## Auswahlregel

Skript: [`tools/phase0/New-CorpusManifest.ps1`](../../tools/phase0/New-CorpusManifest.ps1)

- Pro Boss-Ordner das Log mit der mittleren Dateigröße.
- Bei Ordnern mit mindestens 20 Logs zusätzlich das größte Log.
- Ergebnis: 103 Logs, rund 175 MB.

## Lücken (bleiben bekannt, nicht behebbar mit dem vorhandenen Bestand)

| Lücke | Folge |
| --- | --- |
| Keine `.evtc` (unkomprimiert) | Plain-Pfad nur über Dekompression der `.zevtc` indirekt abgedeckt |
| Kein WvW-Log | `WvWLogic` ohne Referenz |
| Zeitraum 2026-07 bis 2026-11 | Revision 0 und alte Logs fehlen vermutlich |
| Encounter ohne Ordner | Siehe Abschnitt "Lokaler Test-Korpus" in [RUST_MIGRATION_PLAN.md](../RUST_MIGRATION_PLAN.md) |

## Verwendung

Alle Phase-0-Werkzeuge erwarten den Korpus-Wurzelordner als Parameter und lesen die Logs über das Manifest. Die SHA256-Summen erkennen verschobene oder ausgetauschte Dateien.
