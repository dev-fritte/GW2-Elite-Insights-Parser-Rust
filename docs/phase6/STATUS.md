# Phase 6: Schaden, Stand

Stand 2026-11-24, Branch `phase1-ingest`. Code: `rust/ei-core/src/damage.rs`, Vergleich in `rust/ei-parity` (`--damage`).

| Nr. | Arbeitspaket | Stand |
| --- | --- | --- |
| 6.1 | Phasen-Grundgerüst | Vollkampf-Phase und **Breakbar-Phasen** fertig und gemessen (70 von 74 Logs identisch); die Phasen der Encounter-Logik (z. B. "DPS Phase 3") gehören zu Phase 7 und sind nicht portiert |
| 6.2 | Schadensstatistik (Total, Ziel, Phase) | Gesamt und je Ziel über den ganzen Kampf **fertig und gemessen**; je Phase nicht |
| 6.3 | Heilung, Barrier, Defensive, Support | Defensive (Schaden erhalten, Blocks, Ausweichen, Barriere, Strips, Reinigungen), Offensive (Treffer, Krits, Flanken, Unterbrechungen, Kills) und Support (Strips, Reinigungen) **fertig und gemessen**; Heilung und Barriere-Erweiterung gibt es im Korpus nicht (keine Heal-Erweiterung in den Logs) |
| 6.4 | Graphen (DPS usw.) aus Präfixsummen | Schadensgraphen je Sekunde (gesamt, Power, Zustand, erhalten, je Ziel) **fertig und gemessen**; Breakbar-Graphen und Graphen je Phase nicht |
| 6.5 | Zwischenmessung (Gewinn unter Faktor 2: Abbruch) | gemessen mit Einschränkung, siehe unten |
## Was portiert ist

Der Schaden eines Spielers an alle nicht freundlichen Ziele: aus den Datensätzen (`COMBAT`-Zustand; ab arcdps-Build 20260501 ist `is_buff == 0` Direktschaden, sonst Buff-Schaden, das Ergebnis steht in `result`) werden wie in `CombatEventFactory` die Gesundheitsschaden-Ereignisse gebildet und wie in `DamageStatistics.ComputeDamageFrom` summiert: Gesamtschaden, Power (Direktschaden und Buff-Schaden außer Zuständen), Zustandsschaden (Buff-Schaden von Zustands-Buffs, Klassifikation aus der Buff-Tabelle von Phase 4), Strike. Dazu Breakbar-Schaden (Zehntelpunkte) und DPS (auf gerade gerundet wie `Math.Round`).

Encounter-Sonderfall: `LogLogicUtils.NegateDamageAgainstBarrier` (Zwillings-Largos: Nikare und Kenut, und Sabir): Schaden gegen Barriere zählt nicht als Gesundheitsschaden. Im Core als Tabelle nach Trigger-ID (`barrier_negated_species`), also der erste Baustein der Encounter-Logik.

## Messung (103 Korpus-Logs, 950 Spieler)

Vergleich gegen `players[].dpsAll[0]` des C#-Referenz-JSON (nur der Spieler selbst: `actorDamage` und verwandte Felder; Zeitraum ganzer Kampf, Zeitverschiebung und Dauer vom Core (Phase 7; mit `PARITY_REFERENCE_TIMING=1` aus dem Referenz-JSON)):

| Feld | verglichen | Abweichungen |
| --- | ---: | ---: |
| `actorDamage` | 950 | 0 |
| `actorPowerDamage` | 950 | 0 |
| `actorCondiDamage` | 950 | 0 |
| `actorBreakbarDamage` | 950 | 0 |
| `actorDps`, `actorPowerDps`, `actorCondiDps` | je 950 | 0 |

Ohne die Barriere-Regel wichen 40 Spieler in vier Logs (Nikare, Kardinal Sabir) um 15 bis 25 % ab; mit ihr stimmt alles exakt. Aufruf: `ei-parity ... --damage [--verbose]`, mit `PARITY_PLAYERS=1` die abweichenden Spieler.

**Schaden je Ziel (`dpsTargets[Ziel][0]`):** für jedes Ziel der Zielliste (Phase 7, `encounter::log_targets`; bei Xera gehört die zweite Xera zum ersten Ziel). Verglichen wird in den Logs, deren Zielliste samt Reihenfolge der Referenz entspricht (ohne Fake-Ziele und flüchtige Spezies): 4.290 Spieler-Ziel-Paare, 5 Abweichungen bei Gesamt- und Powerschaden (10 bei DPS), alle im Seelenverzehrer-Log (Kampfbeginn 1 ms statt 1.001 ms); Zustandsschaden: 0 Abweichungen.

## 6.3 bis 6.4: Statistikblöcke und Graphen

Code: `rust/ei-core/src/offense.rs` (Ereignisbildung wie `CombatEventFactory`, Zähler wie `OffensiveStatistics`, `DefensePerTargetStatistics`, `SupportStatistics`), `graphs.rs` (Graph wie `ComputeDamageGraph`), `phases.rs` (Breakbar-Phasen wie `GetBreakbarPhases`). Alle Vergleiche laufen mit `ei-parity ... --damage --verbose` (Phasen mit `--offsets`) gegen die erste Phase des Referenz-JSON, mit der Zeitverschiebung und Dauer des Core.

| Block (Referenz-Feld) | verglichen | Abweichungen |
| --- | ---: | ---: |
| Offensive (`statsAll[0]`, 33 Zähler je Spieler) | 31.350 | 508 |
| Offensive je Ziel (`statsTargets[Ziel][0]`) | 141.570 | 279 |
| Defensive (`defenses[0]`, 25 Zähler je Spieler) | 23.750 | 272 |
| Support (`support[0]`, 6 Zähler je Spieler) | 5.700 | 27 |
| Graph Schaden gesamt (`damage1S[0]`) | 950 | 129 |
| Graph Power / Zustand | je 950 | 108 / 78 |
| Graph Schaden erhalten (`damageTaken1S[0]`) | 950 | 41 |
| Graph je Ziel (`targetDamage1S`) | 4.290 | 87 |
| Gesamtschaden mit Minions (`dpsAll[0].damage` und Verwandte) | 2.850 | 272 |
| Breakbar-Phasen (Start, Ende) | 74 Logs | 4 Logs |

Die Abweichungen liegen fast nur in Logs mit bekannter Ursache: Deimos (Zusammenführung mit dem Körper-Gadget), Samarog (`MakeIntoAbsorbed` ändert Schadensereignisse), Seelenverzehrer (Kampfbeginn), Beschworene Verschmelzung (Gadget-Agenten, Schwert und Hand), Drachenleere (Kampfende), Matthias und Faultierion (erhaltener Schaden: encounterspezifische Agenten), dazu bei den Minions die Zuordnung über die Meister-Instanz-ID im Datensatz statt über `FindAgentMaster`. In den Logs ohne diese Sonderfälle stimmen die Blöcke vollständig.

Ausgelassen (je Feld in den JSON-Blöcken, nicht verglichen): Down-Beitrag und Crowd Control (`IsDownedBeforeNext90`), Down-, Tot- und DC-Dauer, Dodge-Zähler (Cast-Ereignisse), Breakbar-Schaden erhalten, Wiederbelebungen und Stunbrüche (Cast- und Stunbruch-Ereignisse), Heilung und Barriere-Erweiterung, Zustandsstrips der Ziele. Die Breakbar-Phasen vergleichen nur Start und Ende (nicht die Namen) und nur in Logs, deren Zielliste der Referenz entspricht; abweichend sind MAMA (1 ms), Seelenverzehrer (Kampfbeginn), Ankka und Kanaxai (Agenten, die die Logik zusammenführt).
## Nicht portiert

- Schaden der Minions über die Meister-Zuordnung von `FindAgentMaster` (der Core nimmt die Meister-Instanz-ID aus dem Datensatz), Begleiter ohne Meister und Schaden je Ziel in Logs mit Fake-Zielen oder flüchtigen Spezies (negative IDs) sowie in Logs mit abweichender Zielliste (Deimos, Qadim u. a.).
- Schaden pro Phase und die Phasen der Encounter-Logik (Phase 7); nur Vollkampf- und Breakbar-Phasen sind portiert.
- Weitere Encounter-Eingriffe in Schadensereignisse (z. B. `Samarog.MakeIntoAbsorbed`) und zusätzlich vom Parser erzeugte Schadensereignisse.
- Logs vor arcdps-Build 20260501 (die alte Ergebnislogik).
- Zeitverschiebung und Dauer bestimmt der Core selbst (Phase 7). Mit seinen Zeiten stimmen die Schadenswerte in den 98 Logs mit richtigem Beginn und Ende exakt; in den 5 anderen Logs weichen 15 von 950 Spielern ab (Gesamtschaden), 42 bei DPS (Rundung der Dauer). Mit den Referenzzeiten (`PARITY_REFERENCE_TIMING=1`) sind alle 950 identisch.

## 6.5: Zwischenmessung

`ei-parity --bench` misst, was der Core je Log rechnet (Metadaten, Spieler, Buffs, Schaden, Statistikblöcke, Graphen, Zielliste, Phasen, Kampfzeit; jeder Aufruf liest das Log neu ein): 103 Logs, Summe **50 s**, Median 227 bis 253 ms, P90 rund 850 ms, Maximum 5,3 s (zwei Läufe).

Die C#-Baseline derselben Logs (`docs/phase0/MEASUREMENTS_0.5.md`, T0): Summe 299 s, Median 2.793 ms, P90 4.490 ms, mit Prozessstart, Laden der API-Dateien (rund 350 ms je Prozess) und der **vollen** Pipeline einschließlich Mechaniken, Schadensmodifikatoren, Combat Replay und JSON-Ausgabe. Das Verhältnis liegt damit bei etwa 6 (Summe) bzw. 11 (Median), aber der Core rechnet bewusst weniger: ihm fehlen die genannten Teile, und der Messlauf liest jedes Log viermal ein. Der Plan-Abbruch (Gewinn unter Faktor 2) wird **nicht** ausgelöst, die Zahl ist aber keine Prognose für den vollständigen Core. Auffällig ist das Maximum von 5,3 s. Vermutlich gehen die Statistikblöcke je Spieler und Ziel einmal über alle Ereignisse (Spieler mal Ziele mal Ereignisse); das ist nicht untersucht. Eine Zusammenfassung zu einem Durchlauf ist der nächste Optimierungsschritt, bevor die Messung wiederholt wird.