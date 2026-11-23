# Rust-Kern für EliteInsights: Bewertung und Arbeitspaket

Basis: "EliteInsights Zielarchitektur & Umsetzungsplan" (Patrick Schmidt). Stand der Prüfung: 2026-11-23. Es wurde noch nichts implementiert.

## 1. Abgleich mit dem Repo

| Aussage im Plan | Befund im Fork |
| --- | --- |
| 123 Encounter-Dateien, 44.000 Zeilen | Stimmt: 123 Dateien, 44.091 Zeilen in `GW2EIEvtcParser/LogLogic` |
| ~2.400 Buffs | Stimmt: 2.390 `new Buff(` |
| ~1.100 Mechaniken | Plausibel: 1.176 Treffer, grobe Zählung |
| 120 Frontend-Dateien | Stimmt |
| 12,7 MB API-JSON | Stimmt: `GW2EIParserCommons/Content` hat 13 MB |
| `CombatItem` als Klasse | Stimmt |
| 59 LINQ-Gruppierungen | Ungefähr: 53 `GroupBy`/`ToDictionary` in `CombatData.cs` |
| Kein AOT/ReadyToRun | Stimmt: nur `AllowUnsafeBlocks` gefunden |
| 11.000 Zeilen Klassenlogik | Nicht verifiziert: `ProfHelper*.cs` allein ~1.100 Zeilen |

Abweichungen:

- Pfade: `GW2EI.Library/GW2EI.Services/{GW2EIEvtcParser,GW2EIBuilders,GW2EIParserCommons}`.
- Gesamtprojekt: ca. 133.000 Zeilen C#.
- Im Repo liegt kein `.evtc`/`.zevtc`; `Regression.cs` setzt sich ohne Datei auf `Inconclusive`. Der Test-Korpus fehlt.
- `GW2EIParserBenchmark` existiert bereits und dient als Startpunkt für die Baseline.

## 2. Pro

1. Ausgangsanalyse passt zum Code (Heap-Objekte, Dictionaries, lineare Zeitfilter, kein AOT).
2. C#-Ausgabe als Wahrheitsquelle; Tests werden nie an die Rust-Ausgabe angepasst.
3. Frontend, JSON-Schema und GUI bleiben unberührt.
4. Buffs und Mechaniken als generierte Daten statt portierter Code.
5. Phase 7 (123 Encounter) ist parallelisierbar.
6. Messen vor Bauen, Abbruchkriterium bei Gewinn unter Faktor 2.
7. Spaltenlayout, CSR-Indizes und Präfixsummen sind etablierte Techniken.
8. Fuzzing, Property-Tests und Performance-Budget in CI.

## 3. Contra und Komplexitäten

**Größte Risiken**

1. **Upstream-Drift (kritisch):** Imperative Encounter-Logik lässt sich nicht automatisch synchronisieren. Ohne Maintainer-Rückhalt läuft der Rust-Kern hinterher.
2. **Fehlender Test-Korpus:** Seltene Encounter und WvW-Logs sind schwer zu beschaffen; Logs enthalten Spieler- und Account-Namen (Datenschutz).
3. **Bit-genaue Parität:** Gleitkomma-Reihenfolge, stabile Sortierung (`StableSort.cs`) und Dictionary-Iterationsreihenfolge; Toleranz 1e-6 deckt keine Ordnungsunterschiede ab.
4. **Buff-Simulator:** stateful, rekursiv, viele Sonderfälle.

**Weitere Komplexitäten**

5. Screenshot-Vergleich der HTML-Ausgabe ist fragil; besser `LogDataDto`-JSON vergleichen.
6. C-ABI/P/Invoke mit großen verschachtelten Ergebnissen ist fehleranfällig; Alternative: CLI-Prozess.
7. Umfang vermutlich unterschätzt (Combat Replay, Extensions, WvW, Uploads, Settings, Lokalisierung).
8. Nicht-Ziele fehlen im Plan (Upload zu dps.report/Wingman/Raidar, Discord-Webhooks, GUI-Features).
9. Zielwerte sind Schätzungen, noch nicht gemessen.
10. Parallelbetrieb bedeutet zwei Codebasen und doppelte CI.
11. Unsafe/Zero-Copy: Struct-Padding, Endianness.
12. Agenten-Risiko: plausible, aber falsche Fachlogik; Gates greifen nur bei ausreichender Korpus-Abdeckung.

### Lokaler Test-Korpus (Stand 2026-11-23)

Die arcdps-Logs liegen lokal unter `arcdps.cbtlogs` (nicht im Repo, nicht in Git ablegen, da Spieler- und Account-Namen enthalten sind).

- 2.531 `.zevtc`, 0 `.evtc` (Plain-evtc-Pfad ist nicht abgedeckt), 3,5 GB, 60 Boss-Ordner.
- Zeitraum 2026-07-06 bis 2026-11-22, also fast sicher nur aktuelle Revision. Rev 0 und alte Logs fehlen vermutlich.
- Sehr ungleich verteilt: 719 Logs in "Die Drachenleere", 148 Standard-Golem, 117 Greer, 117 Dhuum. Viele Bosse haben nur 1 bis 6 Logs, 4 Ordner genau 1.
- Fraenir (22492) liegt doppelt vor (deutscher und englischer Name); Minister Li, Prototyp Vermillion mit je zwei IDs.
- Es fehlt WvW.

**Abgleich Ordner-ID gegen `TargetID` in `LogLogic`** (Stand 2026-11-23, nur über die Ordner-IDs, nicht pro Log geprüft):

Alle 58 Ordner-IDs ließen sich einem `TargetID`-Eintrag zuordnen, und bis auf zwei Gadget-IDs (unten) einer oder mehreren Encounter-Dateien. Ein Ordner entspricht aber einem Boss, nicht einer Datei. Die 123 Dateien enthalten rund 15 `*Instance`-Varianten (Gesamt-Instanz-Logs), die Basisklassen und Hilfsdateien.

Mit Log belegt (Auswahl nach Gruppe):

| Gruppe | Bosse mit Logs |
| --- | --- |
| W1 | Gorseval, Sabetha, Vale Guardian |
| W2 | Slothasor, Matthias, Bandit-Trio (über Berg) |
| W3 | Keep Construct, Xera, Escort (McLeod), Twisted Castle (Haunting Statue) |
| W4 | Cairn, Mursaat Overseer, Samarog, Deimos |
| W5 | Soulless Horror, Dhuum, River (Desmina), Statuen (Broken King, Eater of Souls, Eye of Fate) |
| W6 | Conjured Amalgamate, Twin Largos (Nikare), Qadim |
| W7 | Adina, Sabir, Qadim the Peerless |
| W8 | Greer, Decima (normal und CM), Ura |
| Fraktale | Siax, Ensolyss, MAMA, Skorvald, Artsariiv, Arkk, Kanaxai (CM), Whispering Shadow, Ai, Eparch, Cerus |
| Strikes | Boneskinner, Fraenir, Whisper of Jormag, Icebrood Construct, Voice of the Fallen, Aetherblade (Mai Trin), Xunlai (Ankka), Kaineng (Minister Li, normal und CM), Old Lion's Court (Vermilion, normal und CM), Harvest Temple, Dagda (Cosmic Observatory), Cerus (Temple of Febe), Kela (Guardian's Glade), Vloxx (Nexus of Eternity) |
| Golem | Standard-Golem, Großer Kätzchen-Golem |

Ohne Ordner und damit ohne Log (Lücken für Phase 7):

- Fraktale: Captain Mai Trin (Fraktal-Variante), Deepstone, Solitary Throne (Eternal Tyrant), Lonely Tower Cerus/Deimos-Variante.
- Strikes/Events: Cold War, Freezie (Festival), Soo-Won (OpenWorld), Mordremoth (Story), W1 Spirit Race.
- Convergences: Mount Balrior, Nexus of Eternity, Outer Nayos.
- `WvWLogic`, `Unknown*`-Logik.
- Die `*Instance`-Varianten sind nicht belegt, soweit die Ordner nur Einzel-Boss-Logs enthalten (Stichprobe nötig).

Unklar: Der Ordner "Ätherische Barriere (47188)" entspricht keinem `TargetID`-Eintrag außer einem Gadget. Die Logs im Ordner "Die Drachenleere (43488)" (719) und "Beschworene Verschmelzung (43974)" laufen über Gadget-IDs; welche Encounter-Datei sie tatsächlich auswählt (Harvest Temple beziehungsweise Conjured Amalgamate), ist beim Parsen zu bestätigen.

Konsequenz für 0.1: Korpus nicht blind verwenden, sondern pro Encounter auf ein bis wenige Referenz-Logs kuratieren und die Lücken (WvW, Rev 0, `.evtc`, Encounter ohne Log) in der Coverage-Tabelle ausweisen.

## 4. Empfehlung

Vor dem Rust-Start die C#-Hebel messen: ReadyToRun/Native AOT, Binärsuche statt der 87 linearen Zeitfilter, allokationsfreie Buff-Simulation, vorkompiliertes API-JSON. Rust bleibt dann nur gerechtfertigt, wenn der Speicher-Faktor nötig ist. Abbruchpunkte nach 0.5 und 6.5 verbindlich festlegen.

## 5. Arbeitspaket

Reihenfolge nach Abhängigkeit. Ein Paket entspricht einem PR unter ca. 1.000 Zeilen. Ein Gate muss grün sein, bevor das nächste Paket startet.

### Phase 0: Entscheidungsgrundlage

| Nr. | Arbeitspaket | Ergebnis / Gate |
| --- | --- | --- |
| 0.1 | Test-Korpus beschaffen (je Encounter, WvW, Instanz, Rev 0/1, `.evtc`/`.zevtc`), Datenschutz klären, Ablage außerhalb von Git (LFS oder Release-Asset) | Coverage-Tabelle Encounter × Log |
| 0.2 | C#-Referenz reproduzierbar bauen (festes SDK, `global.json`) | Build in CI grün |
| 0.3 | Referenz-JSON-Generator über den Korpus | Zwei Läufe byte-identisch |
| 0.4 | Baseline messen: Laufzeit, Spitzen-Speicher, CLI-Start; `GW2EIParserBenchmark` erweitern; Phasenprofil | Baseline-Tabelle eingecheckt |
| 0.5 | C#-Schnelltests: R2R/AOT, Binärsuche, allokationsfreie Buff-Simulation | Messung, wie viel C# allein bringt |
| 0.6 | Go/No-Go mit Maintainern: Rust upstream gewünscht? Zielwerte realistisch? | Schriftliche Entscheidung |

### Phase 1 bis 3: Fundament

| Nr. | Arbeitspaket | Gate |
| --- | --- | --- |
| 1.1 | Cargo-Workspace, CI (fmt, clippy pedantic, Tests), MIT-Hinweis | CI grün |
| 1.2 | `evtc-ingest`: Header, Agenten, Skills (mmap, `.zevtc`) | Anzahl und Werte gleich `RawEvtcLog` |
| 1.3 | Ingest der Ereignisse (Zero-Copy, Rev 0/1) | Alle Korpus-Logs identisch |
| 1.4 | Fuzzing und `proptest` für Ingest | 24 h ohne Absturz |
| 2.1 | Spaltenlayout für Ereignisse, dichte Agent-IDs | Round-trip-Test |
| 2.2 | CSR-Indizes pro Agent und Buff, Zeitbereichs-API | Property-Tests gegen naiven Filter |
| 2.3 | Stabile Sortierung wie `StableSort.cs` | Reihenfolge identisch |
| 3.1 | Agent-Aufbereitung: Master/Minion, Englobing, Zeitversatz | Agentenliste identisch |
| 3.2 | Vorverarbeitung (Referenz: `ParsedData/CombatData*.cs`) | Zuordnungen identisch |

### Phase 4: Datenextraktor

| Nr. | Arbeitspaket | Gate |
| --- | --- | --- |
| 4.1 | IDs/GUIDs (`SkillIDs`, `SpeciesIDs`, `EffectGUIDs`) nach Rust-Konstanten | 100 % extrahiert |
| 4.2 | Buffs (2.390) nach TOML/RON, `build.rs` | Jede Definition extrahiert oder begründet |
| 4.3 | Mechaniken (1.176) | wie 4.2 |
| 4.4 | Schadensmodifikatoren, Instant-Cast-Finder | wie 4.2 |
| 4.5 | API-Daten (13 MB) als `rkyv` vorkompilieren | Startzeit gemessen |
| 4.6 | CI-Job: Extraktor gegen Upstream mit automatischem Diff | Drift-Prüfung läuft |

### Phase 5 bis 6: Auswertung

| Nr. | Arbeitspaket | Gate |
| --- | --- | --- |
| 5.1 | Buff-Simulator Duration | Uptimes innerhalb Toleranz |
| 5.2 | Buff-Simulator Intensity | wie 5.1 |
| 5.3 | ID-basierte Simulatoren, Stack-Sonderfälle | wie 5.1 |
| 5.4 | Buff-Graphen | Graphen identisch |
| 5.5 | `dhat`-Test: keine Allokation in der Simulation | Test grün |
| 6.1 | Phasen-Grundgerüst, Präfixsummen | Phasen identisch |
| 6.2 | Schadensstatistik (Total, Ziel, Phase) | Block gleich Referenz |
| 6.3 | Heilung, Barrier, Defensive, Support | je Block gleich Referenz |
| 6.4 | Graphen (DPS usw.) aus Präfixsummen | gleich Referenz |
| 6.5 | Zwischenmessung; Gewinn unter Faktor 2 bedeutet Abbruch | Entscheidung dokumentiert |

### Phase 7: Encounter-Logik

| Nr. | Arbeitspaket | Gate |
| --- | --- | --- |
| 7.0 | Trait `Encounter` mit Basis-Implementierung (Referenz: `LogLogic.cs`) | Generischer Golem-Log paritätisch |
| 7.1 | Klassenlogik: Tabellen plus Sonderfunktionen | Statistikblöcke gleich Referenz |
| 7.2 bis 7.n | Je Encounter ein Paket (Raids, Fractals, Golem, OpenWorld, Convergences, WvW), parallelisierbar | Pro Encounter: Phasen, Mechaniken, Erfolg gleich |

Reihenfolge: zuerst Golem und eine Raid-Gruppe als Muster, dann der Rest in Batches.

### Phase 8 bis 9: Ausgabe und Umstieg

| Nr. | Arbeitspaket | Gate |
| --- | --- | --- |
| 8.1 | Paritäts-Harness: semantischer JSON-Diff, Toleranz 1e-6 | Harness selbst getestet |
| 8.2 | `JsonLog`-Ausgabe (Streaming-Serializer) | Diff leer |
| 8.3 | `LogDataDto` plus Einbetten der 120 Frontend-Dateien | `LogDataDto`-JSON gleich; Screenshot ergänzend |
| 8.4 | CSV, Combat Replay (lazy) | Diff leer |
| 9.1 | CLI mit gleichen Argumenten und Config | Argument-Matrix identisch |
| 9.2 | GUI-Anbindung: zuerst Prozessaufruf, C-ABI nur bei Bedarf | GUI-Smoke-Test |
| 9.3 | Batch-Modus mit Speicherbudget, Arena-Wiederverwendung | Speicher-Gate |
| 9.4 | Performance-Gates in CI (Verschlechterung über 5 % bricht ab), Release-Pipeline | Zielwerte erreicht |
| 9.5 | Parallelbetrieb über einen Release-Zyklus | Keine Abweichungen, `DIVERGENCES.md` freigegeben |

## 6. Nächste Schritte

1. Phase 0 vor jedem Rust-Code; ohne Korpus (0.1) und Go/No-Go (0.6) kein weiterer Aufwand.
2. Maintainer früh fragen, ob ein Rust-Kern upstream gewünscht ist.
3. Abbruchpunkte nach 0.5 und 6.5 verbindlich behandeln.
