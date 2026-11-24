# Phase 5: Buff-System, Stand

Stand 2026-11-24, Branch `phase1-ingest`. Crates: `rust/ei-buffs` (Simulatoren), `rust/ei-core/src/buff_events.rs` (Ereignisschicht), Vergleich in `rust/ei-parity` (`--buffs`).

| Nr. | Arbeitspaket | Stand |
| --- | --- | --- |
| 5.1 | Simulator Duration | Port fertig, gegen C# gemessen (siehe unten) |
| 5.2 | Simulator Intensity | wie 5.1 |
| 5.3 | ID-basierte Simulatoren, Sonderfaelle | **nicht noetig fuer das Korpus**: C# setzt `UseBuffInstanceSimulator = false`, die ID-Simulatoren laufen nie. Sonderfaelle teilweise (siehe Abweichungen) |
| 5.4 | Buff-Graphen | Stapelverlauf je Spieler und Buff verglichen, 98,1 % identisch |
| 5.5 | `dhat`-Test | nicht begonnen; der Port klont Stacks und allokiert |

## Was portiert ist

- `ei-buffs`: `BuffSimulator`, `BuffSimulatorDuration`, `BuffSimulatorIntensity` und die vier Stapel-Logiken (`QueueLogic`, `HealingLogic`, `ForceOverrideLogic`, `OverrideLogic`).
- `buff_events.rs`: aus arcdps-Datensaetzen (ab Build 20260501, Buff-Apply/Remove als Statuswechsel) die Ereignisse je Spieler. Port von `BuffDictionary` (Duplikate von Initial-Buffs und Extensions, `Finalize`: Loeschen bei Spawn, Despawn und letzter Anwesenheit), der Entfernen-Zeit-Korrektur aus `BuffsContainer` (nur bei Stack-IDs), der Regeneration-Sonderbehandlung, Kapazitaet aus `BuffInfo`, Filter `Available` nach Build.

## Messung (103 Korpus-Logs, alle Squad-Spieler)

Vergleich: der Verlauf der Stackzahl (`states` der Spieler-Buffs im C#-Referenz-JSON) gegen den Rust-Verlauf, nach Zusammenfassen gleicher Werte.

| Groesse | Wert |
| --- | ---: |
| verglichene Buff-Graphen (Spieler x Buff, ohne abgeleitete) | 37.449 |
| identisch | 36.750 (98,1 %) |
| abweichend | 699 (1,9 %) |
| Logs mit Fehler | 0 |

Aufruf: `ei-parity --corpus <Wurzel> --manifest docs/phase0/corpus-manifest.tsv --reference <refB1> --content <Content> --buffs [--verbose]`.

**Eingabe aus der Referenz:** Die Zeitverschiebung des Encounters (`logStartOffset`, in C# `GetLogOffset` je Encounter, Phase 7) und die Dauer (`durationMS`) kommen aus dem Referenz-JSON, nicht aus dem Core. Ohne Encounter-Logik ist das nicht ableitbar.

## Bekannte Abweichungen (699 Graphen)

- **Mehrere Agenten je Adresse (ca. 320 Graphen, Buffs 740, 717, 725, 719 ...):** Ein Spieler, dessen Agent im Log zerfaellt oder neu beginnt (C# fuehrt einen `AgentItem` je Instanz-ID), hat in C# eine frueher endende Anwesenheit (z. B. Arkk, Orphay: C# loescht alle Buffs bei 60.396 ms, der Core erst bei 79.354 ms). Der Agenten-Aufbau aus Phase 3 ist nicht portiert.
- **Intensity-Buffs mit Stack-Aenderungen im selben Millisekunden-Fenster (z. B. 1122, 70767, 72975, 69795):** der Verlauf weicht ab dem ersten Abbau von Stacks ab. Ursache nicht untersucht.
- **Einzelfaelle (738, 19426, 76128 ...):** Stackzahl um einen Stack versetzt; vermutet Quellen-/Instanz-Sonderfaelle, nicht untersucht.
- **Abgeleitete Graphen** (negative IDs: Anzahl Boons, Conditions, aktive Minions) sind nicht simuliert und aus dem Vergleich ausgenommen.

## Nicht portiert

Verteilung der Segmente auf Quellen (`BuffDistribution`), umschliessende Agenten, Quellensuche (`BuffSourceFinder`), Waste/Overstack im Vergleich (nur Verlauf verglichen), Profession-Umschreibungen (Weaver, Willbender, Virtuoso, Elementalist), Logs vor Build 20260501 (`Error::UnsupportedBuild`), Uptime-Prozentwerte je Phase (Phasen: 6.1).

`HealingLogic` sortiert in C# mit dem instabilen `List.Sort`; bei gleichem Heilwert bleibt hier die Einfuegereihenfolge (Heilwert der Quelle wird zudem noch nicht aus der Agentenliste gelesen, es ist 0).
