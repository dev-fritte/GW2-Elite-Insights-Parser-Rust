# Rust-Core: Umfang des ersten lauffähigen Stands

Der Plan (`docs/RUST_MIGRATION_PLAN.md`) beschreibt zehn Phasen bis zum vollständigen Ersatz des C#-Kerns (123 Encounter, Buff-System, Statistik, HTML/JSON-Ausgabe). Das sind viele Monate Arbeit und lässt sich nicht in einem Zug erledigen.
Dieses Dokument legt fest, was **der erste lauffähige Core** enthält, damit er überprüfbar ist und keine Behauptung ohne Beleg bleibt.

## Definition "lauffähig"

Ein Rust-Programm (`ei-cli`), das ein `.evtc` oder `.zevtc` einliest und ein JSON ausgibt, dessen Felder gegen das C#-Referenz-JSON (`docs/phase0/reference-hashes.tsv`, lokale JSONs unter `D:\_Projekte\GW2EI-Phase0\refB1`) auf allen 103 Korpus-Logs geprüft sind. Was nicht geprüft ist, steht nicht in der Ausgabe.

## Enthalten (Ziel dieser Etappe)

| Baustein | Phase im Plan | Prüfung |
| --- | --- | --- |
| Ingest: Header, Agenten, Skills, Ereignisse | 1 | Orakel-Hashes, 103/103 (fertig) |
| Kernmodell: Ereignisse mit dichten Agent-IDs, CSR-Indizes pro Quelle und Ziel, Zeitbereichsabfrage per Binärsuche | 2 | Property-Tests gegen den naiven Filter |
| Metadaten aus State-Change-Ereignissen: Build, Revision, Karte, Sprache, Region, Fraktalstufe, Trigger-ID | 3 (Teilmenge) | Feldvergleich mit dem Referenz-JSON |
| Spielerliste (Name, Account, Gruppe, Profession) | 3 (Teilmenge) | Feldvergleich, Abweichungen werden gezählt und benannt |
| CLI und JSON-Ausgabe der obigen Felder | 8 (Teilmenge) | Feldvergleich über `ei-parity` |

## Nicht enthalten (bewusst, mit Grund)

| Was fehlt | Grund |
| --- | --- |
| Encounter-Logik (Name, Erfolg, Phasen, Ziele, Mechaniken) | 123 Dateien, Phase 7; ohne sie gibt es kein `fightName`, `success`, `targets` |
| Buff-System, Uptimes, Graphen | Phase 5, größter Block der Auswertung |
| Schaden, Heilung, Defensive, Support | Phase 6, braucht Ziele und Phasen |
| API-Daten (Skill-/Buff-Namen, Icons) | Phase 4, 13 MB JSON-Daten |
| HTML, CSV, Combat Replay | Phase 8 |
| Uhrzeit-Felder (`timeStart`, `timeEnd`, `duration`) | das C#-Referenz-JSON formatiert sie in der lokalen Zeitzone des Messrechners (`ToLocalTime`), das Rust-Ergebnis müsste dieselbe Zone erraten; Unix-Zeitstempel werden stattdessen ausgegeben (ungeprüft gegen das JSON, aber gegen die Rohereignisse) |

## Folgerung

Der Core ist nach dieser Etappe ein **Prototyp mit Parität auf Metadaten und Spielerliste**, nicht ein Ersatz des C#-Parsers. Die nächsten Etappen laut Plan sind Phase 4 (Daten-Extraktor), 5 (Buffs), 6 (Statistik), 7 (Encounter). Jede davon hat ein eigenes Paritäts-Gate gegen die vorhandene Referenz.
