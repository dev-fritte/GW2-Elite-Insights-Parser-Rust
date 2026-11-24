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
- **Encounter-spezifische Überschreibungen von `GetLogOffset`:** Cairn (Verlust des Spawn-Schutzes; der Rückfall über das Ende des ersten Zaubers fehlt), Xera, Sabir, Arkk, Escort (McLeod der Stille), Dhuum.
- **Kampfende, Tod-Fall:** Zeit des letzten Schadens der Spieler am Hauptziel, höchstens der Tod, plus 10 ms, höchstens das Log-Ende (`SetSuccessByDeath`); sonst die letzte Zeit des Ziels (Fehlschlag).

## Messung (103 Korpus-Logs, 26 Encounter)

Vergleich mit `logStartOffset` und `durationMS` des C#-Referenz-JSON:

| Größe | identisch |
| --- | ---: |
| Kampfbeginn | 73 von 103 (vorher, nur generisch: 63) |
| Dauer | 27 von 103 |

Aufruf: `ei-parity ... --offsets`; er listet je Encounter die Logs mit Abweichung und Beispiele.

Die Dauer weicht oft nur um 2 bis 20 ms ab (z. B. Cairn 79.711 statt 79.709, Faultierion 201.159 statt 201.158). Ursache nicht untersucht; verdächtig ist die Auswahl der "Spieler-Schaden" (Schaden von Begleitern über `IsMasterOrSelf`, hier nur über die Instanz-ID des Herrn angenähert) und die Aufteilung der Agenten.

## Nicht portiert (Ursachen der Abweichungen)

- **Kampfbeginn, weitere Überschreibungen** (30 Logs): BanditTrio (Berg), Deimos, River (Desmina), TempleOfFebe (Cerus), Ensolyss, SuperKodan, SpiritRace (Ätherische Barriere, dort auch die Überschreibung der Trigger-ID 47188 durch 15415), ConjuredAmalgamate (Beschworene Verschmelzung, flüchtige Spezies), HarvestTemple (Drachenleere), Qadim, GuardiansGlade (Kela), NexusOfEternity (Vloxx), OldLionsCourt, Skorvald, Kanaxai, Greer, Ura, Albtraum (MAMA/Siax-Familie) und weitere.
- **Kampfende:** Erfolg über Truhen-Gadget (`ChestID` je Encounter), Kampfausstieg (`CombatExit`), und die `CheckSuccess`-Überschreibungen der Encounter.
- **Phasen, Zielliste, Schaden je Ziel und Phase, freundliche NPCs, Modus (CM), Mechaniken, Erfolgsbedingungen.**

## Nächste Schritte

Je Encounter ein Paket (Kampfbeginn, Ende, Phasen, Ziele), nach Häufigkeit im Korpus; jeweils mit dem Vergleich von `--offsets` und den Phasen aus dem Referenz-JSON als Gate. Danach können Zeitverschiebung und Dauer aus dem Referenz-JSON in den Phasen 5 und 6 durch die Werte des Cores ersetzt werden (derzeit stammen sie aus der Referenz).
