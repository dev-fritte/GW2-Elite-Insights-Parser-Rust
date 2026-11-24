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

## Zielliste und Vollkampf-Phase

`encounter::log_targets` bestimmt die Ziele wie `LogLogic.ComputeLogTargets`: Agenten der Spezies, die die Logik nennt (Tabelle `ei_data::targets`, erzeugt aus `docs/phase7/targets.tsv` durch `GW2EIParserReference --dump-targets` und `tools/gen/Gen-Targets.ps1`), zusammengefasst nach `RegroupSameAgentsAndDetermineTeams` (gleiche Instanz-ID, Spezies, Name und Master im selben Squad-Kampffenster), sortiert nach erstem Auftreten und den Sortierschlüsseln der Logik.

Vergleich der Spezies-Reihenfolge mit `targets` der Referenz (ohne Fake-Ziele und ohne die negativen, flüchtigen Spezies): **95 von 103 Logs identisch** (`ei-parity --offsets`, Zeile `target lists`). Abweichend: Ensolyss, Eparch (Attrappen werden in C# als ignorierte Spezies markiert), Qadim, Skorvald (Zusammenführungen der Logik), Cerus, Dagda und Kela (Reihenfolge bei gleichem ersten Auftreten) sowie ein Deimos-Log.

Die Vollkampf-Phase (`phases[0]`) ist in allen 103 Referenzlogs Start 0 bis `durationMS` (Typ `Encounter`); der Name ist in 100 Logs `Full Fight`, in 3 ein encounterspezifischer (`Final Number`). Ihr Ende ist damit durch die Messung der Dauer oben abgedeckt. Teilphasen (Breakbar, Phasen je Encounter) sind nicht portiert.
## Phasen der Encounter-Logik

Code: `rust/ei-core/src/subphases.rs`. Portiert sind die Bausteine `GetSubPhasesByInvul` (Buff-Folge mit Entfernen am Halter, Mindestabstand 10 ms, Schlussmarke beim letzten Auftreten des Ziels), `GetPhasesByHealthPercent`, `AddTarget` (nur wenn das Ziel im Fenster aktiv ist), `OverrideTimes` (Phase nach Kampfeintritt, Tod und Despawn der Ziele anpassen) und die Nachbearbeitung aus `LogData` (Phasen ohne Ziele und unter 1.000 ms fallen weg, Sortierung nach Beginn und Länge), dazu die Phasen von: Gorseval, Vale Guardian, Sabetha (Mini-Bosse in umgekehrter Reihenfolge), Faultierion (Narkolepsie-Casts), Samarog, Skorvald, Siax, MAMA, Ensolyss, Mursaat-Aufseher, Kardinal Sabir (Casts), Eisbrut-Konstrukt und die Golems (Zahlen nach Lebensprozent plus Prozentschritte).

Vergleich (`ei-parity --offsets`, Zeile `sub phases`): Start und Ende der Unterphasen und einfachen Zeitrahmen (`SubPhase` und `TimeFrame` ohne Breakbar-Phasen; die "In Combat"-Phasen der Golems aus der Sicht des Aufnehmenden sind nicht portiert und werden ausgelassen), sortiert; nur in Logs, deren Zielliste der Referenz entspricht. Ergebnis: **21 von 21 Logs identisch**. Die Namen der Phasen werden nicht berechnet.

Die übrigen rund 70 Logs mit Phasen (45 Encounter) haben jeweils eigene Phasenlogik: Casts und Effekte (Qadim, Dhuum, Xera, Matthias, Deimos, Statue der Dunkelheit, Zwillings-Largos, Kardinal Adina, Fraenir, Kodan-Brüder, Mai Trin, Ankka, Kela, Decima, Cerus, Harvest Temple, Beschworene Verschmelzung u. a.), Lebensprozent und Zusammenführungen von Zielen (Bandit-Trio, Arkk, Old Lion's Court, Ura, Greer). Sie sind **nicht portiert**; ohne ihre Zielliste (siehe Abweichungen oben) ist ein Vergleich dort auch nicht möglich.
## Nicht portiert (Ursachen der Abweichungen)

- **Kampfbeginn:** nur Seelenverzehrer (keine eigene Regel in C#; der Agent hat in Rust den ersten Datensatz bei 1 ms, die Referenz beginnt bei 1.001 ms; Ursache nicht gefunden). Die Bandit-Trio-Regel lässt die Prüfung der Trash-Mobs weg, weil die C#-Funktion IsInBoundingBox den Kasten auf einen Punkt verkleinert (sie vergleicht mit der ersten Ecke statt mit beiden) und nie zutrifft.
- **Kampfende:** Deimos (Zusammenschluss bei 10 %, Angriffsziele), Drachenleere (HarvestTemple.CheckSuccess), Kampfausstieg (CombatExit, nur Xera und Qadim als Rückfall) und die übrigen CheckSuccess-Überschreibungen (zuletzt rund 45 Dateien; Encounter ohne Log im Korpus sind ungeprüft).
- **Phasen, Zielliste, Schaden je Ziel und Phase, freundliche NPCs, Modus (CM), Mechaniken, Erfolgsbedingungen.**

## Nächste Schritte

Je Encounter ein Paket (Kampfbeginn, Ende, Phasen, Ziele), nach Häufigkeit im Korpus; jeweils mit dem Vergleich von `--offsets` und den Phasen aus dem Referenz-JSON als Gate. Zeitverschiebung und Dauer der Phasen 5 und 6 kommen inzwischen aus dem Core.
