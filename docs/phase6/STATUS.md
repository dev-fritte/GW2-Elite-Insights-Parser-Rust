# Phase 6: Schaden, Stand

Stand 2026-11-24, Branch `phase1-ingest`. Code: `rust/ei-core/src/damage.rs`, Vergleich in `rust/ei-parity` (`--damage`).

| Nr. | Arbeitspaket | Stand |
| --- | --- | --- |
| 6.1 | Phasen-Grundgerüst, Präfixsummen | **nicht begonnen**; die Phasen (z. B. "DPS Phase 3") kommen aus der Encounter-Logik (Phase 7), nur "Full Fight" ist generisch |
| 6.2 | Schadensstatistik (Total, Ziel, Phase) | Gesamtschaden je Spieler über den ganzen Kampf **fertig und gemessen**; je Ziel und je Phase nicht |
| 6.3 | Heilung, Barrier, Defensive, Support | nicht begonnen |
| 6.4 | Graphen (DPS usw.) aus Präfixsummen | nicht begonnen |
| 6.5 | Zwischenmessung (Gewinn unter Faktor 2: Abbruch) | **nicht durchgeführt**, siehe unten |

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

## Nicht portiert

- Schaden der Minions und Begleiter (`damage`, `powerDamage` ohne `actor`) und Schaden je Ziel in Logs mit Fake-Zielen oder flüchtigen Spezies (negative IDs) sowie in Logs mit abweichender Zielliste (Deimos, Qadim u. a.).
- Schaden pro Phase und die Phasen selbst (6.1).
- Weitere Encounter-Eingriffe in Schadensereignisse (z. B. `Samarog.MakeIntoAbsorbed`) und zusätzlich vom Parser erzeugte Schadensereignisse.
- Logs vor arcdps-Build 20260501 (die alte Ergebnislogik).
- Zeitverschiebung und Dauer bestimmt der Core selbst (Phase 7). Mit seinen Zeiten stimmen die Schadenswerte in den 98 Logs mit richtigem Beginn und Ende exakt; in den 5 anderen Logs weichen 15 von 950 Spielern ab (Gesamtschaden), 42 bei DPS (Rundung der Dauer). Mit den Referenzzeiten (`PARITY_REFERENCE_TIMING=1`) sind alle 950 identisch.

## 6.5: Zwischenmessung

Die Messung "Gewinn gegenüber C#" ist **nicht** aussagekräftig möglich: der Core berechnet nur Metadaten, Spieler, Buffs der Spieler und Schaden, das C#-Programm die volle Pipeline. Ein ehrlicher Vergleich braucht entweder ein C#-Programm, das nur diese Teile rechnet, oder den vollständigen Core. Die Entscheidung des Plans (Abbruch bei Faktor unter 2) ist damit nicht getroffen.
