# Phase 5: Buff-System, Stand

Stand 2026-11-24, Branch `phase1-ingest`. Crates: `rust/ei-buffs` (Simulatoren), `rust/ei-core/src/buff_events.rs` (Ereignisschicht), Vergleich in `rust/ei-parity` (`--buffs`).

| Nr. | Arbeitspaket | Stand |
| --- | --- | --- |
| 5.1 | Simulator Duration | Port fertig, gegen C# gemessen (siehe unten) |
| 5.2 | Simulator Intensity | wie 5.1 |
| 5.3 | ID-basierte Simulatoren, Sonderfaelle | **nicht noetig fuer das Korpus**: C# setzt `UseBuffInstanceSimulator = false`, die ID-Simulatoren laufen nie. Sonderfaelle teilweise (siehe Abweichungen) |
| 5.4 | Buff-Graphen | Stapelverlauf je Spieler und Buff verglichen, 99,95 % identisch |
| 5.5 | `dhat`-Test | nicht begonnen; der Port klont Stacks und allokiert |

## Was portiert ist

- `ei-buffs`: `BuffSimulator`, `BuffSimulatorDuration`, `BuffSimulatorIntensity` und die vier Stapel-Logiken (`QueueLogic`, `HealingLogic`, `ForceOverrideLogic`, `OverrideLogic`).
- `buff_events.rs`: aus arcdps-Datensaetzen (ab Build 20260501, Buff-Apply/Remove als Statuswechsel) die Ereignisse je Spieler. Port von `BuffDictionary` (Duplikate von Initial-Buffs und Extensions, `Finalize`: Loeschen bei Spawn, Despawn und letzter Anwesenheit), der Entfernen-Zeit-Korrektur aus `BuffsContainer` (nur bei Stack-IDs), der Regeneration-Sonderbehandlung, Kapazitaet aus `BuffInfo`, Filter `Available` nach Build.

## Messung (103 Korpus-Logs, alle Squad-Spieler)

Vergleich: der Verlauf der Stackzahl (`states` der Spieler-Buffs im C#-Referenz-JSON) gegen den Rust-Verlauf, nach Zusammenfassen gleicher Werte. ZusÃ¤tzlich die Gegenrichtung: Buffs, die der Core hat, die Referenz aber nicht auflistet (ohne versteckte Buffs, wie das C#-JSON).

| GrÃ¶ÃŸe | Wert |
| --- | ---: |
| verglichene Buff-Graphen (Spieler x Buff, inkl. benutzerdefinierter) | 37.495 |
| identisch | 37.478 (99,95 %) |
| abweichend | 17 |
| Buffs nur im Core | 0 |
| Logs mit Fehler | 0 |

Verlauf der Messung, jeweils mit dem Fund der Analyse:

| Stand | abweichend | behoben durch |
| --- | ---: | --- |
| erste Ereignisschicht | 32.429 | Zeitverschiebung des Encounters (aus der Referenz Ã¼bernommen) |
| + Entfernen-Zeit-Korrektur (`BuffsContainer`) | 1.062 | Stacks, die mit voller Originaldauer entfernt werden |
| ohne abgeleitete Graphen | 699 | Vergleich nur fÃ¼r echte Buffs |
| + `OffsetBuffExtensionEvents` | 75 | arcdps meldet die neue Dauer ab dem Original-Stack; die Extension wird um die gelaufene Zeit korrigiert, sonst wÃ¤hlte der Simulator den falschen Stack |
| + DatensÃ¤tze ohne Adresse, nur mit Instanz-ID | 23 | Despawns dieses arcdps-Builds haben `src_agent = 0` und nur eine Instanz-ID; C# ordnet sie dem Agenten mit dieser ID zu (`orphanedSrcInstidCombatItems`) |
| + Gegenrichtung geprÃ¼ft | 23 (201 Buffs nur im Core) | der Vergleich ging bisher nur von den Buffs der Referenz aus |
| + Weaver-Attunements und Dual-Buffs | 18 (194 nur im Core) | `TransformWeaverAttunements` fÃ¼r Weaver, `RemoveDualBuffs` fÃ¼r andere Elementalisten (`buff_events/weaver.rs`) |
| + unbekannte Nahrung und Verbesserungen | 17 | `BuffsContainer` erzeugt Buffs fÃ¼r unbekannte VerbrauchsgÃ¼ter der Kategorie der bekannten (`buff_events/custom.rs`) |
| + Filter der Gegenrichtung wie im C#-JSON | 17 (0 nur im Core) | versteckte Buffs und Buffs ohne Zeit im Log stehen nicht im JSON |

Aufruf: `ei-parity --corpus <Wurzel> --manifest docs/phase0/corpus-manifest.tsv --reference <refB1> --content <Content> --buffs [--verbose]`; mit `PARITY_PLAYERS=1` die Abweichungen je Spieler, mit `PARITY_EXTRAS=1` die Buffs nur im Core.

**Zeiten des Encounters:** Zeitverschiebung und Dauer bestimmt jetzt der Core selbst (`encounter.rs`, Phase 7). In 90 von 103 Logs sind beide identisch mit C#; dort stimmen die Buff-Verläufe wie oben (17 Abweichungen, alle in Matthias und bei der Regeneration). In den 13 Logs, in denen Beginn oder Ende noch fehlen (Berg, Deimos, Beschworene Verschmelzung, Drachenleere, Seelenverzehrer, Auge des Schicksals, Eparch), weichen die Verläufe ab: gesamt 3.485 von 37.495 Graphen bei Core-Zeiten. Mit der Umgebungsvariable `PARITY_REFERENCE_TIMING=1` nimmt `ei-parity` Verschiebung und Dauer wieder aus dem Referenz-JSON (dann gelten die Zahlen oben).

## Verbleibende Abweichungen (17 Graphen)

- **Matthias, Opferung (15 Graphen; Buffs 738, 740, 19426, 737, Spieler HÃ¶rski und Antali So Dreamy):** C# zeigt doppelt so viele Stacks. Ursache im C#-Code gefunden: `Matthias.FindSacrifices` legt einen eigenen Agenten "Sacrificed N <Spieler>" an und kopiert die verbleibenden Ereignisse des Spielers auf ihn (`AgentManipulationHelper.RedirectDamageAndCopyRemainingFromSrcToDst`); in der Referenz taucht die Quelle "Sacrificed 1 Nevara Sonnentau" auf. Das ist Encounter-Logik (Phase 7), im Core nicht vorhanden.
- **Regeneration, 2 ms (Buff 718; 2 Graphen):** das Ende des Buffs liegt in der Referenz 2 ms spÃ¤ter. Ursache nicht gefunden; die Sortier- und Aktivierungslogik von `HealingLogic` (instabiles `List.Sort`, Heilwert der Quelle ist 0) kommt in Frage.
- **Abgeleitete Graphen** (Anzahl Boons, Conditions, aktive Minions, und die von Mesmer-, Ranger-Helfern erzeugten) sind nicht simuliert und aus dem Vergleich ausgenommen, soweit der Core den Buff nicht kennt.
## Nicht portiert

Verteilung der Segmente auf Quellen (`BuffDistribution`), umschliessende Agenten, Quellensuche (`BuffSourceFinder`), Waste/Overstack im Vergleich (nur Verlauf verglichen), Profession-Umschreibungen auÃŸer Weaver und Elementalist (Willbender, Virtuoso, Scourge, Mesmer, Troubadour, Ranger), Logs vor Build 20260501 (`Error::UnsupportedBuild`), Uptime-Prozentwerte je Phase (Phasen: 6.1).

`HealingLogic` sortiert in C# mit dem instabilen `List.Sort`; bei gleichem Heilwert bleibt hier die Einfuegereihenfolge (Heilwert der Quelle wird zudem noch nicht aus der Agentenliste gelesen, es ist 0).
