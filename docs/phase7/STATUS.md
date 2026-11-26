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
- **Encounter-spezifische Überschreibungen von `GetLogOffset`** (23 Encounter): Cairn (Rückfall über das Ende des ersten Zaubers fehlt), Xera, Sabir, Arkk, Escort (McLeod), Dhuum, River, Ensolyss, MAMA, Siax, Skorvald, Kanaxai, Greer, Ura, Kela, Vloxx, Statue des Eises, Statue des Todes (Seelenverzehrer: Peasants oder erster Schaden des Seelenfressers), Qadim, Old Lions Court, SpiritRace (nur die Zeit, nicht die Trigger-ID-Überschreibung), Cerus. Gemeinsame Bausteine: `GetLogOffsetBySpawn`, `GetLogOffsetByInvulnStart`, `GetEnterCombatForStart`, `GetFirstDamageEventTime`.
- **Kampfende (`CheckSuccess`):** Reihenfolge wie in C#: bei den Raid-Wings zuerst der Belohnungsdatensatz (`CurrentRaidReward`), dann das Truhen-Gadget (`FindChestGadgets`: Gadget mit Hitbox 100 an der Truhenposition, ohne Bewegung; Zeit = erstes Auftreten), dann der Tod des Hauptziels (Zeit des letzten Spielerschadens, höchstens der Tod), sonst ein Fehlschlag zur letzten Zeit des Ziels. Bei den Icebrood-Strikes zuerst der Belohnungsdatensatz der Truhen. Ein Erfolg endet 10 ms nach der Zeit, höchstens am Log-Ende. Eigene Regeln: Ankka, Minister Li, Kanaxai, Ai (Sunqua), Mai Trin (Aetherblade), Old Lions Court, Golems, Twisted Castle und Spirit Race (alter Raid-Belohnungsdatensatz).

## Messung (103 Korpus-Logs, 26 Encounter)

Vergleich mit `logStartOffset` und `durationMS` des C#-Referenz-JSON:

| Größe | identisch |
| --- | ---: |
| Kampfbeginn | 103 von 103 (nur generisch: 63) |
| Dauer | **103 von 103** (nur Tod-Fall: 27; mit Truhe: 68; mit Belohnungsdatensätzen und eigenen Regeln: 103) |

Aufruf: `ei-parity ... --offsets`; er listet je Encounter die Logs mit Abweichung und Beispiele.

Die letzte Abweichung (ein Cerus-Log mit 1 ms) kam daher, dass die Raid-Encounter nach Secret of the Obscure (Febe, Kosmisches Observatorium) mit ihrem eigenen Belohnungsdatensatz (Typ 29453, `CheckPostEODRewardSuccess`) enden, nicht beim Tod des Ziels; Rust nutzt ihn jetzt für diese beiden Encounter. Die übrigen Encounter derselben Basisklasse (Kaineng, Löwenhof, Ätherklingen, Xunlai, Nexus, Wächterhain) stimmen ohne den Datensatz überein und sind nicht umgestellt.

## Zielliste und Vollkampf-Phase

`encounter::log_targets` bestimmt die Ziele wie `LogLogic.ComputeLogTargets`: Agenten der Spezies, die die Logik nennt (Tabelle `ei_data::targets`, erzeugt aus `docs/phase7/targets.tsv` durch `GW2EIParserReference --dump-targets` und `tools/gen/Gen-Targets.ps1`), zusammengefasst nach `RegroupSameAgentsAndDetermineTeams` (gleiche Instanz-ID, Spezies, Name und Master im selben Squad-Kampffenster), sortiert nach erstem Auftreten und den Sortierschlüsseln der Logik.

Vergleich der Spezies-Reihenfolge mit `targets` der Referenz (ohne Fake-Ziele und ohne die negativen, flüchtigen Spezies): **101 von 103 Logs identisch** (`ei-parity --offsets`, Zeile `target lists`). Ausgeschlossen werden wie in C# die falschen Ensolysse (nie 7 Mio. Leben), die Attrappen des Eparchs (nie ein Zauber außer Waffe ziehen und wegstecken), die Flux-Anomalien von Skorvald mit Determined und die Pyre-Wächter Qadims an bekannten Positionen. Die permanenten Verkörperungen von Cerus (die ersten Agenten jeder Verkörperung, die innerhalb von 50 ms nach Cerus auftauchen) sind wie in C# (`TempleOfFebe.EIEvtcParse`) eine eigene, negative Spezies und stehen am Ende der Liste; der Vergleich lässt sie wie die der Referenz aus. Abweichend: Dagda und Kela (Reihenfolge bei gleichem ersten Auftreten). Die Zeiten, in denen ein Agent bekannt ist (`encounter::spans`, `UpdateAgentData`), enthalten jetzt auch die Datensätze, die den Agenten nur über die Instanz-ID nennen (Quelle oder Ziel ohne Adresse; `orphanedSrcInstidCombatItems`): ein solcher Datensatz gehört dem Agenten mit dieser ID, der 300 ms davor oder danach bekannt ist. Das ordnete die elf Schatten von Cerus richtig (Beginn bis 2 ms früher, Ende bis 1 ms später) und beseitigte alle Abweichungen von Schaden, Bedingungsschaden, Kraftschaden und Schaden pro Sekunde je Ziel; die Breakbar-Phasen stimmen in 77 von 77 Logs.

Die Vollkampf-Phase (`phases[0]`) ist in allen 103 Referenzlogs Start 0 bis `durationMS` (Typ `Encounter`); der Name ist in 100 Logs `Full Fight`, in 3 ein encounterspezifischer (`Final Number`). Ihr Ende ist damit durch die Messung der Dauer oben abgedeckt. Teilphasen (Breakbar, Phasen je Encounter) sind nicht portiert.
## Phasen der Encounter-Logik

Code: `rust/ei-core/src/subphases.rs`. Portiert sind die Bausteine `GetSubPhasesByInvul` (Buff-Folge mit Entfernen am Halter, Mindestabstand 10 ms, Schlussmarke beim letzten Auftreten des Ziels), `GetPhasesByHealthPercent`, `AddTarget` (nur wenn das Ziel im Fenster aktiv ist), `OverrideTimes` (Phase nach Kampfeintritt, Tod und Despawn der Ziele anpassen) und die Nachbearbeitung aus `LogData` (Phasen ohne Ziele und unter 1.000 ms fallen weg, Sortierung nach Beginn und Länge), dazu die Phasen von: Gorseval, Vale Guardian, Sabetha (Mini-Bosse in umgekehrter Reihenfolge), Faultierion (Narkolepsie-Casts), Samarog, Skorvald, Siax, MAMA, Ensolyss, Mursaat-Aufseher, Kardinal Sabir (Casts), Eisbrut-Konstrukt und die Golems (Zahlen nach Lebensprozent plus Prozentschritte), außerdem Ura (Normalmodus, bei 70 % und 40 %; der Herausforderungsmodus mit der geheilten Phase ist ausgelassen), Decima (Normalmodus, Nova-Schild; das Schild beim Enrage zählt nicht), Cerus (Splits der Verkörperungen, Enraged Smash), Seelenloser Schrecken (Heulender Tod), Artsariiv (Klone), Matthias (Buffs der Elemente), Geflüster des Jormag (Teleports) und Old Lion's Court (Schilde der Prototypen), Eparch (Herausforderungsmodus), Bandit-Trio, Flüsternder Schatten, Qadim (Pyre-Start, Lampen über die Marker), Dhuum (Vorereignis, Seelenteilungen), Fraenir, Statue der Dunkelheit (Augen), Kardinal Adina (Hände aus den Angriffszielen), Super Kodan Brothers (ein Log weicht um 1 ms ab), Zwillings-Largos (Rückfall über den ersten Treffer eines Spielers), Beschworene Verschmelzung (Arme aus den Angriffszielen), Kela (Sturmphasen über das Eingraben), Elementar-Ai (nur ohne dunkle Ai), Festenkonstrukt (Haupt-, Brenn-, Vorbrenn- und Restphasen), Nexus der Ewigkeit, Kanaxai (Herausforderungsmodus) und Dagda (Gruppen der Gepeinigten).

Vergleich (`ei-parity --offsets`, Zeile `sub phases`): Start und Ende der Unterphasen und einfachen Zeitrahmen (`SubPhase` und `TimeFrame` ohne Breakbar-Phasen; die "In Combat"-Phasen der Golems aus der Sicht des Aufnehmenden sind nicht portiert und werden ausgelassen), sortiert; nur in Logs, deren Zielliste der Referenz entspricht. Ergebnis: **66 von 67 Logs identisch** (abweichend: Kodan mit 1 ms) (Logs mit abweichender Zielliste stehen in einer zweiten Zeile, dort werden nur die Zeiten verglichen). Die Namen der Phasen werden nicht berechnet.

Die übrigen rund 25 Logs mit Phasen (rund 20 Encounter) haben jeweils eigene Phasenlogik: Xera und Deimos (Vorereignis mit flüchtigen Zielen), Mai Trin, Ankka, Decima im Herausforderungsmodus, Harvest Temple, Kaineng, Arkk (die Blüten sind Trash-Agenten) und Greer. Sie sind **nicht portiert**; ohne ihre Zielliste (siehe Abweichungen oben) ist ein Vergleich dort auch nicht möglich.
## Nicht portiert (Ursachen der Abweichungen)

- **Kampfbeginn:** keine Abweichung im Korpus. Die Bandit-Trio-Regel lässt die Prüfung der Trash-Mobs weg, weil die C#-Funktion IsInBoundingBox den Kasten auf einen Punkt verkleinert (sie vergleicht mit der ersten Ecke statt mit beiden) und nie zutrifft.
- **Kampfende:** Deimos nur ohne die Regeln des Zusammenschlusses bei 10 % (`Deimos.CheckSuccess` mit Angriffszielen; die Ziel-Zeit reicht wie in C# bis zum Log-Ende, das deckt den Fehlschlag im Korpus), Drachenleere nur ohne Truhe (`HarvestTemple.CheckSuccess`: Truhen-Gadget mit Hitbox 2 an der Truhenposition, sonst Fehlschlag am Log-Ende, das die Logik für die Ziele setzt; die verzögerte Belohnung über Soo-Won und Liftoff-Casts ist nicht portiert), Kampfausstieg (CombatExit, nur Xera und Qadim als Rückfall) und die übrigen CheckSuccess-Überschreibungen (zuletzt rund 45 Dateien; Encounter ohne Log im Korpus sind ungeprüft).
- **Phasen, Zielliste, Schaden je Ziel und Phase, freundliche NPCs, Modus (CM), Mechaniken, Erfolgsbedingungen.**

## Nächste Schritte

Je Encounter ein Paket (Kampfbeginn, Ende, Phasen, Ziele), nach Häufigkeit im Korpus; jeweils mit dem Vergleich von `--offsets` und den Phasen aus dem Referenz-JSON als Gate. Zeitverschiebung und Dauer der Phasen 5 und 6 kommen inzwischen aus dem Core.
