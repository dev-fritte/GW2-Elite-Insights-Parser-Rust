# Phase 7: Encounter-Logik, Stand

Stand 2026-11-24, Branch `phase1-ingest`. Code: `rust/ei-core/src/encounter.rs`, Vergleich in `rust/ei-parity` (`--offsets`).

**Phase 7 ist nur begonnen.** Der Plan sieht 123 Encounter-Dateien mit rund 44.000 Zeilen vor (7.0 Basis, 7.1 Klassenlogik, 7.2 bis 7.n je Encounter). Portiert ist der Anfang der Basis: wann der Kampf beginnt und, nur teilweise, wann er endet.

| Nr. | Arbeitspaket | Stand |
| --- | --- | --- |
| 7.0 | Basis `LogLogic` (generisch) | Kampfbeginn (`GetLogOffset`) generisch und für 6 Encounter; Kampfende nur der Tod-Fall; Phasen, Ziele, Erfolg, Trigger-ID-Überschreibungen **nicht** |
| 7.1 | Klassenlogik | nicht begonnen |
| 7.2 bis 7.n | je Encounter | nicht begonnen, bis auf `GetLogOffset` von Cairn, Xera, Sabir, Arkk, Escort (McLeod), Dhuum und die Barriere-Regel aus Phase 6 |

## Was portiert ist

- **Generischer Kampfbeginn:** `LogLogic.GetLogOffset` mit `GetEnterCombatTime`, `GetPostLogStartNPCUpdateDamageEventTime`. Instanz-Logs: 0. Sonst die Zeit, zu der das Hauptziel nach dem `LogNPCUpdate`-Datensatz in den Kampf ging.
- **Encounter-spezifische Überschreibungen von `GetLogOffset`** (22 Encounter): Cairn (Rückfall über das Ende des ersten Zaubers fehlt), Xera, Sabir, Arkk, Escort (McLeod), Dhuum, River, Ensolyss, MAMA, Siax, Skorvald, Kanaxai, Greer, Ura, Kela, Vloxx, Statue des Eises, Qadim, Old Lions Court, SpiritRace (nur die Zeit, nicht die Trigger-ID-Überschreibung), Cerus. Gemeinsame Bausteine: `GetLogOffsetBySpawn`, `GetLogOffsetByInvulnStart`, `GetEnterCombatForStart`, `GetFirstDamageEventTime`.
- **Kampfende (`CheckSuccess`):** Reihenfolge wie in C#: bei den Raid-Wings zuerst der Belohnungsdatensatz (`CurrentRaidReward`), dann das Truhen-Gadget (`FindChestGadgets`: Gadget mit Hitbox 100 an der Truhenposition, ohne Bewegung; Zeit = erstes Auftreten), dann der Tod des Hauptziels (Zeit des letzten Spielerschadens, höchstens der Tod), sonst ein Fehlschlag zur letzten Zeit des Ziels. Bei den Icebrood-Strikes zuerst der Belohnungsdatensatz der Truhen. Ein Erfolg endet 10 ms nach der Zeit, höchstens am Log-Ende. Eigene Regeln: Ankka, Minister Li, Kanaxai, Ai (Sunqua), Mai Trin (Aetherblade), Old Lions Court, Golems, Twisted Castle und Spirit Race (alter Raid-Belohnungsdatensatz).

## Messung (103 Korpus-Logs, 26 Encounter)

Vergleich mit `logStartOffset` und `durationMS` des C#-Referenz-JSON:

| Größe | identisch |
| --- | ---: |
| Kampfbeginn | 102 von 103 (nur generisch: 63) |
| Dauer | 98 von 103 (nur Tod-Fall: 27; mit Truhe: 68; mit Belohnungsdatensätzen und eigenen Regeln: 98) |

Aufruf: `ei-parity ... --offsets`; er listet je Encounter die Logs mit Abweichung und Beispiele.

Die 5 Logs mit abweichender Dauer: Deimos (CM-Log, 1 Log; Kampfende über den Zusammenschluss bei 10 % und Angriffsziel-Ereignisse), Drachenleere (2 Logs; Kampfende über Angriffsziele von Soo-Won und Liftoff-Casts), Seelenverzehrer (1 Log; der Kampfbeginn liegt 1.000 ms daneben) und ein Cerus-Log mit 1 ms Abweichung (beide Ursachen nicht gefunden).

## Nicht portiert (Ursachen der Abweichungen)

- **Kampfbeginn:** nur Seelenverzehrer (keine eigene Regel in C#; der Agent hat in Rust den ersten Datensatz bei 1 ms, die Referenz beginnt bei 1.001 ms; Ursache nicht gefunden). Die Bandit-Trio-Regel lässt die Prüfung der Trash-Mobs weg, weil die C#-Funktion IsInBoundingBox den Kasten auf einen Punkt verkleinert (sie vergleicht mit der ersten Ecke statt mit beiden) und nie zutrifft.
- **Kampfende:** Deimos (Zusammenschluss bei 10 %, Angriffsziele), Drachenleere (HarvestTemple.CheckSuccess), Kampfausstieg (CombatExit, nur Xera und Qadim als Rückfall) und die übrigen CheckSuccess-Überschreibungen (zuletzt rund 45 Dateien; Encounter ohne Log im Korpus sind ungeprüft).
- **Phasen, Zielliste, Schaden je Ziel und Phase, freundliche NPCs, Modus (CM), Mechaniken, Erfolgsbedingungen.**

## Nächste Schritte

Je Encounter ein Paket (Kampfbeginn, Ende, Phasen, Ziele), nach Häufigkeit im Korpus; jeweils mit dem Vergleich von `--offsets` und den Phasen aus dem Referenz-JSON als Gate. Zeitverschiebung und Dauer der Phasen 5 und 6 kommen inzwischen aus dem Core.
