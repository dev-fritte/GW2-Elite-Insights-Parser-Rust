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
- **Kampfende, Tod-Fall:** Zeit des letzten Schadens der Spieler am Hauptziel, höchstens der Tod, plus 10 ms, höchstens das Log-Ende (`SetSuccessByDeath`); sonst die letzte Zeit des Ziels (Fehlschlag).

## Messung (103 Korpus-Logs, 26 Encounter)

Vergleich mit `logStartOffset` und `durationMS` des C#-Referenz-JSON:

| Größe | identisch |
| --- | ---: |
| Kampfbeginn | 94 von 103 (nur generisch: 63) |
| Dauer | 38 von 103 |

Aufruf: `ei-parity ... --offsets`; er listet je Encounter die Logs mit Abweichung und Beispiele.

Bei Cairn liegt der Tod des Ziels bei 79.713 ms, der Core addiert 10 ms, C# kommt auf 79.711; woher die 2 ms kommen, habe ich nicht gefunden. Die Dauer weicht oft nur um 2 bis 20 ms ab (z. B. Cairn 79.711 statt 79.709, Faultierion 201.159 statt 201.158). Ursache nicht untersucht; verdächtig ist die Auswahl der "Spieler-Schaden" (Schaden von Begleitern über `IsMasterOrSelf`, hier nur über die Instanz-ID des Herrn angenähert) und die Aufteilung der Agenten.

## Nicht portiert (Ursachen der Abweichungen)

- **Kampfbeginn, 9 Logs in 5 Encounter:** BanditTrio (Berg; braucht Positionsdatensätze und die Trash-Mob-Liste), Deimos (Gefangener, Grün-Auswahl), ConjuredAmalgamate (Beschworene Verschmelzung; Effekt-GUID `CAArmSmash`, flüchtige Spezies), HarvestTemple (Drachenleere; Effekte) und Seelenverzehrer (keine eigene Regel, der generische Beginn liegt 1.000 ms daneben; Ursache nicht gefunden).
- **Kampfende:** Erfolg über Truhen-Gadget (`ChestID` je Encounter), Kampfausstieg (`CombatExit`), und die `CheckSuccess`-Überschreibungen der Encounter.
- **Phasen, Zielliste, Schaden je Ziel und Phase, freundliche NPCs, Modus (CM), Mechaniken, Erfolgsbedingungen.**

## Nächste Schritte

Je Encounter ein Paket (Kampfbeginn, Ende, Phasen, Ziele), nach Häufigkeit im Korpus; jeweils mit dem Vergleich von `--offsets` und den Phasen aus dem Referenz-JSON als Gate. Danach können Zeitverschiebung und Dauer aus dem Referenz-JSON in den Phasen 5 und 6 durch die Werte des Cores ersetzt werden (derzeit stammen sie aus der Referenz).
