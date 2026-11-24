# Phase 5: Buff-System, Stand

Stand 2026-11-24, Branch `phase1-ingest`. Crate: `rust/ei-buffs`.

| Nr. | Arbeitspaket | Stand |
| --- | --- | --- |
| 5.1 | Simulator Duration | Port fertig, **nur gegen handgerechnete Faelle geprueft** |
| 5.2 | Simulator Intensity | wie 5.1 |
| 5.3 | ID-basierte Simulatoren, Sonderfaelle | nicht begonnen |
| 5.4 | Buff-Graphen | nicht begonnen |
| 5.5 | `dhat`-Test (keine Allokation) | nicht begonnen; der Port klont Stacks und allokiert |

## Was portiert ist

`BuffSimulator`, `BuffSimulatorDuration`, `BuffSimulatorIntensity` (ohne Stack-IDs) und die vier Stapel-Logiken (`QueueLogic`, `HealingLogic`, `ForceOverrideLogic`, `OverrideLogic`). Eingabe sind `SimEvent`s (Apply, Extend, RemoveAll, RemoveSingle, Activate, Deactivate), Ausgabe `SimItem`-Segmente, Waste und Overstack. `uptime()` rechnet Duration-Buffs als Zeitanteil und Intensity-Buffs als mittlere Stackzahl. Die Stack-Typen kommen aus `ei-data` (`BUFFS`).

## Was **nicht** belegt ist

Das Gate des Plans (Uptimes innerhalb Toleranz gegen die Referenz) ist **nicht** erreicht. Dafuer fehlt die Schicht, die aus den arcdps-Ereignissen `SimEvent`s macht: Erkennen von Apply/Remove/Extension (C# `CombatEventFactory`, `BuffInfoSolver`, Initial-Buffs, Entfernen unpassender Remove-Ereignisse, Instanz-Ids), die Phasen (6.1) und die Auswahl der Buffs je Log (`Available`). Bis dahin pruefen nur elf Unit-Tests das Verhalten, ihre Erwartungen habe ich aus dem C#-Code abgeleitet, nicht gegen C# gemessen.

Bekannte Abweichung: `HealingLogic` sortiert in C# mit dem instabilen `List.Sort`; bei gleichem Heilwert bleibt hier die Einfuegereihenfolge.

Nicht portiert: Verteilung der Segmente auf Quellen (`BuffDistribution`), umschliessende Agenten (Englobing).

## Naechster Schritt

Ereignisse zu `SimEvent`s (inklusive Orakel: C# dumpt je Spieler und Buff die `GenerationSimulation` fuer ein paar Logs), dann Vergleich der Segmente.
