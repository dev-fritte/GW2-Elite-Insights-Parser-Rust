# Phase 4: Datenextraktor, Stand

Stand 2026-11-23, Branch `phase1-ingest`. Crate: `rust/ei-data`. Nichts in `rust/ei-data/src/generated` ist von Hand geschrieben.

| Nr. | Arbeitspaket | Stand |
| --- | --- | --- |
| 4.1 | IDs/GUIDs nach Rust-Konstanten | **erledigt**: `tools/gen/Gen-Ids.ps1` |
| 4.2 | Buffs | **erledigt**: C#-Dump per Reflection, `tools/gen/Gen-Buffs.ps1` |
| 4.3 | Mechaniken | nicht begonnen |
| 4.4 | Schadensmodifikatoren, Instant-Cast-Finder | nicht begonnen |
| 4.5 | API-Daten als `rkyv` | nicht begonnen |
| 4.6 | CI-Drift-Pruefung | nicht begonnen |

## 4.1 IDs und GUIDs

Quelle: `SkillIDs.cs`, `SpeciesIDs.cs`, `EffectGUIDs.cs`, `MarkerGUIDs.cs`. Der Generator liest zeilenweise; jede Deklaration, die er nicht versteht, bricht den Lauf ab, ebenso doppelte Rust-Namen.

| Ausgabe | Anzahl | Gegenprobe (grep im C#-Quelltext) |
| --- | ---: | --- |
| `skill_ids` (`i64`) | 5.827 | 5.794 `public const long` + 33 `internal const long` |
| `species_ids::species` (`i32`) | 136 | `const int` in `SpeciesIDs` |
| `species_ids::target_id` / `minion_id` / `chest_id` | 770 / 253 / 30 | Enum-Mitglieder, Verweise auf andere Mitglieder und Konstanten aufgeloest |
| `effect_guids` | 1.100 | 1.100 `static readonly Guid` |
| `marker_guids` | 32 | 34 Deklarationen, davon 2 `IReadOnlyCollection<Guid>` (Gruppierungen, nicht extrahiert) |

GUIDs sind die 16 Bytes der 32 Hexziffern in Lesereihenfolge, nicht das .NET-`Guid`-Speicherlayout. Beim Einsatz gegen Effekt-Ereignisse muss die Byte-Reihenfolge noch gegen echte Logs geprueft werden; das ist **nicht** geschehen.

`SpeciesIDs` enthaelt zusaetzlich `Vector3`-Truhenpositionen; diese sind nicht extrahiert (kein ID-Wert, Phase 7).

## 4.2 Buffs

Die Buff-Definitionen sind Konstruktoraufrufe mit aufgeloesten Konstanten und Bild-Links. Statt den Quelltext zu parsen, liest `GW2EIParserReference --dump-data <Ordner>` (`DataDump.cs`) alle statischen Buff-Listen per Reflection und schreibt `docs/phase4/buffs.tsv` (Name, ID, Klassifikation, Quellen, Stapelart, Kapazitaet, GW2- und arcdps-Build-Bereiche, Icon-Link). `Gen-Buffs.ps1` erzeugt daraus `BUFFS` (2.372 Eintraege).

Gegenprobe: Der Quelltext enthaelt 2.378 nicht auskommentierte Zeilen mit `new Buff(`; 6 davon (Ingenieur-Kits, `EngineerHelper.cs`) stehen in einem Blockkommentar und sind nicht aktiv. Die Namensmengen von Quelle und Dump stimmen sonst ueberein. Der Plan nannte 2.390 Buffs; die Zahl war eine Schaetzung.

Nicht enthalten: Buffs, die erst zur Laufzeit erzeugt werden (`CreateCustomBuff`), und die Filterung nach Log (`Available(combatData)`, `GetCurrentBuffs`); diese Logik gehoert in Phase 5.

## Warum 4.3 und 4.4 nicht auf dieselbe Weise gehen

Mechaniken und Schadensmodifikatoren bestehen zu einem grossen Teil aus Delegaten (Bedingungen, Filter, Berechnungen). Delegaten lassen sich nicht als Daten extrahieren, sie muessen als Code portiert werden. Extrahierbar waeren nur die deklarativen Felder (Name, ID, Typ, Build-Bereich). Das lohnt sich erst zusammen mit der Auswertung (Phase 5 und 6) und ist deshalb zurueckgestellt.
