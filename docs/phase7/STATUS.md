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
| Kampfbeginn | 94 von 103 (nur generisch: 63) |
| Dauer | 90 von 103 (nur Tod-Fall: 27; mit Truhe: 68; mit Belohnungsdatensätzen und eigenen Regeln: 90) |

Aufruf: `ei-parity ... --offsets`; er listet je Encounter die Logs mit Abweichung und Beispiele.

Die 13 Logs mit abweichender Dauer: 9 davon liegen in den fünf Encounter ohne passenden Kampfbeginn (Berg, Deimos, Beschworene Verschmelzung, Drachenleere, Seelenverzehrer), dazu Auge des Schicksals (2 Logs, `HasIntersectingLastGrasps`), Eparch (CM-Erkennung) und ein Cerus-Log mit 1 ms Abweichung (Ursache nicht gefunden).

## Nicht portiert (Ursachen der Abweichungen)

- **Kampfbeginn, 9 Logs in 5 Encounter:** BanditTrio (Berg; braucht Positionsdatensätze und die Trash-Mob-Liste), Deimos (Gefangener, Grün-Auswahl), ConjuredAmalgamate (Beschworene Verschmelzung; Effekt-GUID `CAArmSmash`, flüchtige Spezies), HarvestTemple (Drachenleere; Effekte) und Seelenverzehrer (keine eigene Regel, der generische Beginn liegt 1.000 ms daneben; Ursache nicht gefunden).
- **Kampfende:** Kampfausstieg (`CombatExit`, nur Xera und Qadim als Rückfall), Eparch, Auge des Schicksals und die übrigen `CheckSuccess`-Überschreibungen (zuletzt rund 45 Dateien; Encounter ohne Log im Korpus sind ungeprüft).
- **Phasen, Zielliste, Schaden je Ziel und Phase, freundliche NPCs, Modus (CM), Mechaniken, Erfolgsbedingungen.**

## Nächste Schritte

Je Encounter ein Paket (Kampfbeginn, Ende, Phasen, Ziele), nach Häufigkeit im Korpus; jeweils mit dem Vergleich von `--offsets` und den Phasen aus dem Referenz-JSON als Gate. Danach können Zeitverschiebung und Dauer aus dem Referenz-JSON in den Phasen 5 und 6 durch die Werte des Cores ersetzt werden (derzeit stammen sie aus der Referenz).
