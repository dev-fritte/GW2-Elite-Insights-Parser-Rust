# Rust-Core: Stand des ersten lauffähigen Prototyps

Stand 2026-11-23, Branch `phase1-ingest`. Umfang und Abgrenzung stehen in [CORE_SCOPE.md](CORE_SCOPE.md). Dieses Dokument hält fest, was **belegt** ist und was nicht.

## Was läuft

`rust/` ist ein Cargo-Workspace mit diesen Crates:

| Crate | Aufgabe | Prüfung |
| --- | --- | --- |
| `evtc-ingest` | Liest `.evtc`, `.zevtc`, `.evtc.zip`: Header, Agenten, Skills, Ereignisse (Revision 0 und 1) | Orakel, 103/103 Logs |
| `ei-core` | Ereignismodell (dichte Agent-IDs, CSR-Indizes, Zeitbereichsabfrage), Port des Ereignisfilters aus `ParseCombatList`, Metadaten, Spieler | Orakel Schicht B, Property-Tests, Referenz-JSON |
| `ei-cli` | Programm `ei`: Log einlesen, JSON ausgeben | siehe unten |
| `evtc-oracle` | Vergleicht Rust-Ergebnisse mit der C#-Orakel-Tabelle | 103 Logs, 19 Spalten |
| `ei-parity` | Vergleicht das Rust-JSON feldweise mit dem C#-Referenz-JSON | 103 Logs |

Aufruf des Programms:

```powershell
cargo run --release -p ei-cli -- <log.zevtc> --content GW2EI.Library/GW2EI.Services/GW2EIParserCommons/Content --pretty
```

Beispielausgabe (gekürzt): `arcVersion`, `arcRevision`, `triggerID`, `isInstanceLog`, `mapID`, `gW2Build`, `language`, `languageID`, `region`, `fractalScale`, `players` (Name, Account, Gruppe, Profession). Zusätzlich `coreLogStartUnix`/`coreLogEndUnix` (Unix-Sekunden der Squad-Kampfereignisse; das C#-JSON hat diese Felder nicht, sondern lokalzeitformatierte Texte).

## Belegte Parität (103 Korpus-Logs)

**Rohdaten und Ereignisfilter** gegen das C#-Orakel (`docs/phase1/oracle-hashes.tsv`): **0 Abweichungen in 19 Spalten** auf allen Logs. Das sind Header, Anzahl und Hash der Agenten, Skills und Ereignisse, und alles, was `ParseCombatList` behält (Anzahl, Hash, Trigger-ID, Karte, Log-Start-Offset, Log-Ende, GW2-Build).

**Felder des JSON** gegen das C#-Referenz-JSON (`ei-parity`):

| Feld | Logs verglichen | Abweichungen |
| --- | ---: | ---: |
| `arcVersion`, `arcRevision` | 103 | 0 |
| `isInstanceLog`, `mapID`, `gW2Build` | 103 | 0 |
| `language`, `languageID`, `region`, `fractalScale` | 103 | 0 |
| Squad-Start und -Ende (als Unix-Zeit gegen `timeStartStd`/`timeEndStd`) | 103 | 0 |
| `triggerID` | 103 | **2** |
| `players` (Squad-Spieler als Menge aus Name, Account, Gruppe, Profession) | 103 | **8** |

### Bekannte Abweichungen

**`triggerID` (2 Logs):** Beide sind "Ätherische Barriere" (Header-ID 47188). Die C#-Logik für den Geisterrennen-Abschnitt (`SpiritRace`) ersetzt die Trigger-ID durch 15415 (`WallOfGhosts`); ähnliche Überschreibungen gibt es in `TwistedCastle` und `River`. Das ist Encounter-Logik (Phase 7) und im Core nicht enthalten.

**`players` (8 von 103 Logs):**

- Freundliche NPCs (Gruppe 51, `friendlyNPC=true`) werden im C#-JSON von der Encounter-Logik zur Spielerliste hinzugefügt. Sie werden im Vergleich ausgeklammert, nicht nachgebildet.
- In 8 Logs weicht die Menge der echten Spieler um genau einen Eintrag ab (ein Spieler zu viel, oder ein Spieler mit anderer Spezialisierung oder Gruppe). Vermutete Ursache ist die nicht portierte Behandlung in `AgentManipulationHelper` (Aufteilen von Spielern nach Spezialisierung und Gruppe). In einem Log (Albtraum-Oratuss) hat der überzählige Spieler nur wenige Ereignisse (u. a. ChangeDead und Initial-Buffs); die Regel, nach der C# ihn ausschließt, habe ich nicht gefunden. Die Ursache ist nicht verifiziert.
- Die C#-Reihenfolge der Liste (kulturabhängiger Vergleich) wird nicht nachgebildet; der Vergleich ignoriert die Reihenfolge.

Die Zeitfelder `timeStart`, `timeEnd`, `duration` des C#-JSON sind lokalzeitabhängig (`ToLocalTime`) und werden deshalb nicht ausgegeben (siehe CORE_SCOPE.md).

## Laufzeit und Speicher des Prototyps

Messung: `tools/rust/Measure-Ei.ps1`, ein Prozess pro Log, 103 Logs, zwei Läufe, `ei.exe` im Release-Profil.

| Größe | Rust-Core (zwei Läufe) | C#-Baseline (`BASELINE.md`) |
| --- | --- | --- |
| Wall pro Log, Median | 31 ms | 2.793 ms (volle Pipeline) |
| Wall pro Log, P90 | 47 / 61 ms | 4.490 ms |
| Wall pro Log, Maximum | 78 / 79 ms | 7.598 ms |
| Spitzen-Arbeitsspeicher, Median | 14,6 MB | 304 MB |
| Spitzen-Arbeitsspeicher, Maximum | 95,9 MB | 878 MB |
| Summe über 103 Logs | 3,7 s | 299 s |

**Diese Zahlen sind nicht vergleichbar mit der vollen C#-Pipeline.** Der Rust-Core macht nur Ingest, Ereignisfilter, Metadaten und Spielerliste, nicht Buffs, Schaden, Phasen, Mechaniken und JSON-Ausgabe. Der passendere Vergleich ist die C#-Parse-Phase (Median 749 ms, `BASELINE.md`), die aber auch mehr tut (Aufbau von `CombatData`, Agenten-Aufbereitung). Der Speicherwert ist eine untere Schätzung (alle 2 ms abgefragt). Was die Zahlen belegen: Einlesen, Dekomprimieren (zlib-rs) und Filtern kosten in Rust nur wenige Millisekunden pro Log. Ob der Gewinn bei den teuren Phasen (Buffs, Statistik) ähnlich ausfällt, ist damit **nicht** belegt.

## Qualitätssicherung

- `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` (pedantic) und `cargo test --workspace --locked` laufen lokal grün (60 Tests, darunter Property-Tests für Reader und Indizes).
- `unsafe` ist im gesamten Workspace verboten.
- Der GitHub-Workflow `rust-check.yml` ist angelegt, aber **noch nicht auf GitHub gelaufen**.
- Nicht ausgeführt: 24 Stunden Fuzzing (Plan, Phase 1.4). Stattdessen Property-Tests mit zufälligen Bytes.

## Was noch fehlt für einen Ersatz des C#-Parsers

In der Reihenfolge des Plans: Phase 4 (Daten-Extraktor für Buffs, Mechaniken, IDs; API-Daten als `rkyv`), Phase 5 (Buff-System), Phase 6 (Schaden, Heilung, Defensive, Präfixsummen), Phase 7 (123 Encounter, Trigger-IDs, freundliche NPCs, Erfolg, Phasen), Phase 8 (vollständiges `JsonLog`, HTML, CSV), Phase 9 (CLI-Kompatibilität, GUI-Anbindung). Nichts davon ist begonnen, bis auf die genannten Teilmengen.
