# Phase 5: Buff-System, Stand

Stand 2026-11-24, Branch `phase1-ingest`. Crates: `rust/ei-buffs` (Simulatoren), `rust/ei-core/src/buff_events.rs` (Ereignisschicht), Vergleich in `rust/ei-parity` (`--buffs`).

| Nr. | Arbeitspaket | Stand |
| --- | --- | --- |
| 5.1 | Simulator Duration | Port fertig, gegen C# gemessen (siehe unten) |
| 5.2 | Simulator Intensity | wie 5.1 |
| 5.3 | ID-basierte Simulatoren, Sonderfaelle | **nicht noetig fuer das Korpus**: C# setzt `UseBuffInstanceSimulator = false`, die ID-Simulatoren laufen nie. Sonderfaelle teilweise (siehe Abweichungen) |
| 5.4 | Buff-Graphen | Stapelverlauf je Spieler und Buff verglichen, 99,94 % identisch |
| 5.5 | `dhat`-Test | nicht begonnen; der Port klont Stacks und allokiert |

## Was portiert ist

- `ei-buffs`: `BuffSimulator`, `BuffSimulatorDuration`, `BuffSimulatorIntensity` und die vier Stapel-Logiken (`QueueLogic`, `HealingLogic`, `ForceOverrideLogic`, `OverrideLogic`).
- `buff_events.rs`: aus arcdps-Datensaetzen (ab Build 20260501, Buff-Apply/Remove als Statuswechsel) die Ereignisse je Spieler. Port von `BuffDictionary` (Duplikate von Initial-Buffs und Extensions, `Finalize`: Loeschen bei Spawn, Despawn und letzter Anwesenheit), der Entfernen-Zeit-Korrektur aus `BuffsContainer` (nur bei Stack-IDs), der Regeneration-Sonderbehandlung, Kapazitaet aus `BuffInfo`, Filter `Available` nach Build.

## Messung (103 Korpus-Logs, alle Squad-Spieler)

Vergleich: der Verlauf der Stackzahl (`states` der Spieler-Buffs im C#-Referenz-JSON) gegen den Rust-Verlauf, nach Zusammenfassen gleicher Werte.

| GrÃ¶ÃŸe | Wert |
| --- | ---: |
| verglichene Buff-Graphen (Spieler x Buff, ohne abgeleitete) | 37.449 |
| identisch | 37.426 (99,94 %) |
| abweichend | 23 |
| Logs mit Fehler | 0 |

Verlauf der Messung, jeweils mit dem Fund der Analyse:

| Stand | abweichend | behoben durch |
| --- | ---: | --- |
| erste Ereignisschicht | 32.429 | Zeitverschiebung des Encounters (aus der Referenz Ã¼bernommen) |
| + Entfernen-Zeit-Korrektur (`BuffsContainer`) | 1.062 | Stacks, die mit voller Originaldauer entfernt werden |
| ohne abgeleitete Graphen | 699 | Vergleich nur fÃ¼r echte Buffs |
| + `OffsetBuffExtensionEvents` | 75 | arcdps meldet die neue Dauer ab dem Original-Stack; die Extension wird um die gelaufene Zeit korrigiert, sonst wÃ¤hlte der Simulator den falschen Stack |
| + DatensÃ¤tze ohne Adresse, nur mit Instanz-ID | 23 | Despawns dieses arcdps-Builds haben `src_agent = 0` und nur eine Instanz-ID; C# ordnet sie dem Agenten mit dieser ID zu (`orphanedSrcInstidCombatItems`) |

Aufruf: `ei-parity --corpus <Wurzel> --manifest docs/phase0/corpus-manifest.tsv --reference <refB1> --content <Content> --buffs [--verbose]`; mit der Umgebungsvariable `PARITY_PLAYERS=1` zusÃ¤tzlich die Abweichungen je Spieler.

**Eingabe aus der Referenz:** Die Zeitverschiebung des Encounters (`logStartOffset`, in C# `GetLogOffset` je Encounter, Phase 7) und die Dauer (`durationMS`) kommen aus dem Referenz-JSON, nicht aus dem Core. Ohne Encounter-Logik ist das nicht ableitbar.

## Verbleibende Abweichungen (23 Graphen)

- **Verdoppelte Stackzahlen (Buffs 738, 740, 19426, 737; Matthias-Logs, Spieler HÃ¶rski und Antali So Dreamy; 13 Graphen):** C# zeigt doppelt so viele Stacks. Vermutung: der Spieler hat zwei Agenten, die C# zusammenfÃ¼hrt (`RegroupSameAgentsAndDetermineTeams`), sodass beide EreignisstrÃ¶me zÃ¤hlen. Nicht verifiziert.
- **Weaver-Attunements (5580, 5575; 3 Graphen):** `WeaverHelper.TransformWeaverAttunements` schreibt die Ereignisse um; nicht portiert.
- **Zwei Millisekunden Versatz (718, 41166; 4 Graphen):** Regeneration und ein weiterer Buff enden bzw. beginnen 2 ms frÃ¼her oder spÃ¤ter; Ursache nicht untersucht.
- **Buff vor Log-Beginn (67803; 1 Graph):** Referenz beginnt bei -8.432 ms, der Core liefert nichts. Nicht untersucht.
- **Abgeleitete Graphen** (negative IDs: Anzahl Boons, Conditions, aktive Minions) sind nicht simuliert und aus dem Vergleich ausgenommen.
## Nicht portiert

Verteilung der Segmente auf Quellen (`BuffDistribution`), umschliessende Agenten, Quellensuche (`BuffSourceFinder`), Waste/Overstack im Vergleich (nur Verlauf verglichen), Profession-Umschreibungen (Weaver, Willbender, Virtuoso, Elementalist), Logs vor Build 20260501 (`Error::UnsupportedBuild`), Uptime-Prozentwerte je Phase (Phasen: 6.1).

`HealingLogic` sortiert in C# mit dem instabilen `List.Sort`; bei gleichem Heilwert bleibt hier die Einfuegereihenfolge (Heilwert der Quelle wird zudem noch nicht aus der Agentenliste gelesen, es ist 0).
