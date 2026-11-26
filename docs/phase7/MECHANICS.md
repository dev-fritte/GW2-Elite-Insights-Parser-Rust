# Mechaniken, Stand

Stand 2026-11-25, Branch `phase1-ingest`. Code: `rust/ei-core/src/mechanics.rs`, Vergleich in `rust/ei-parity` (`--damage`, Zeile `mechanics (generic)`).

## Umfang in C#

Die Mechaniken stehen in `MechanicList` der Encounter-Logiken (`LogLogic`-Konstruktor für die allgemeinen, je Klasse für die encounterspezifischen). Sie bestehen aus einem Typ (Schaden, Cast, Buff, Status, Effekt, Missile, Spawn, Erfolgsbedingung), den Skill-IDs, Namen, Darstellung und häufig einem Lambda als Prüfung (`UsingChecker`, `UsingEnable`, Zeitbegrenzer, Untermechaniken). Die Referenz-JSONs enthalten 1.966 Mechanik-Einträge in 726 verschiedenen Namen; aufgeführt wird nur, was Ereignisse hat.

Lambdas lassen sich nicht wie die Buffs oder Ziele aus dem Quelltext oder per Reflexion als Daten ausgeben. Der Plan-Punkt 4.3 ("Mechaniken als generierte Daten") gilt deshalb nur für den Teil ohne Prüfung; der Rest muss je Mechanik als Code oder als eigener Auswertertyp portiert werden.

## Portiert

Die allgemeinen Mechaniken, die jedes Log hat (`LogLogic`-Konstruktor):

| Name | Quelle |
| --- | --- |
| `Dead`, `Downed`, `Got up` | Statuswechsel Tod, Niedergang, Aufstehen der Spieler |
| `DC`, `Resp` | Despawn und Spawn der Spieler; Datensätze ohne Adresse, aber mit Instanz-ID, gehören dem Spieler mit dieser ID, der um die Zeit bekannt ist (`orphanedSrcInstidCombatItems`) |
| `Res` | Beginn der Animation des Wiederbelebens (Skill 1066); eine Animation, die in derselben Millisekunde endet, zählt nicht |
| `Miss.Qck`, `Miss.Alac` | Buff-Anwendungen (Schnelligkeit, Tempo) der Gruppe: je Geber der Untergruppe die Folgen innerhalb von 15 ms; wer fehlt, obwohl er lebt und verbunden ist, bekommt den Eintrag. Quickness: Lively-Lute-Wellen innerhalb von 1.100 ms nach dem Zauberbeginn zählen als eine Folge (auch ein Zauber, der vor dem Log begann, nur mit Ende-Datensatz). Nur Spieler, die am Kampf teilnahmen (Schaden, Schaden erlitten oder Buff an andere); die anderen fehlen in der Spielerliste von C# |
| `Knck.Dwn`, `Knck.Pll`, `Flt`, `Lnch`, `Lckt`, `Wtr.Flt.Snk` | Crowd Control (Ergebnis 12) auf Spieler, nach dem generischen arcdps-Skill |

Zeiten sind relativ zum Kampfbeginn, nur Ereignisse im Kampf zählen.

## Encounter-Mechaniken aus Daten

`GW2EIParserReference --dump-mechanics docs/phase7/mechanics.tsv` liest per Reflexion die Mechanik-Listen aller Logiken (1.642 Einträge: Typ, Skill-IDs, Kurzname, Abklingzeit, Builds und die Merker, ob Lambda, Untermechanik, Zeitbegrenzer, Aktivierungsbedingung oder Minions daran hängen). `tools/gen/Gen-Mechanics.ps1` erzeugt daraus `ei-data/src/generated/mechanics.rs` mit den Einträgen **ohne** Zusatz, soweit der Typ portiert ist:

| Typ | Einträge (ohne Zusatz / alle) | Auswertung |
| --- | ---: | --- |
| `PlayerDstHealthDamageHitMechanic` | 592 / 669 | Treffer (direkt: normal, kritisch, Streifschuss; Buff-Schaden: Ergebnis 14 bis 18) mit der Skill-ID auf einen Spieler |
| `PlayerDstBuffApplyMechanic` | 327 / 386 | Buff-Anwendung (auch Anfangszustand) auf einen Spieler |
| `EnemyCastStartMechanic` | 91 | Beginn eines animierten Zaubers eines Gegners |
| `EnemyCastEndMechanic` | 5 | Ende (Beginn + tatsächliche Dauer) eines animierten Zaubers eines Gegners |
| `EnemyDstBuffApplyMechanic` | 84 | Buff-Anwendung (auch Anfangszustand) auf einen Gegner |
| `SpawnMechanic` | 18 | erstes Auftreten der Agenten der Spezies (Gegner) |
| `EnemyDstBuffRemoveMechanic`, `PlayerDstBuffRemoveMechanic` | 5, 4 | Entfernen aller Stapel (Datensatz "Entfernen alle"; betroffen ist die Quelle des Datensatzes, nicht das Ziel) von einem Gegner, von einem Spieler |
| `PlayerSrcHealthDamageHitMechanic` | 10 | Treffer eines Spielers mit der Fertigkeit |
| `PlayerSrcBuffApplyMechanic` | 4 | Buff-Anwendung eines Spielers (Quelle ist der Spieler, bei Dienern ihr Besitzer; nur Prüfung der Spezies des Ziels) |
| `PlayerDstEffectMechanic`, `PlayerSrcEffectMechanic` | 26, 14 | Effekt (GUID über die Zuordnung Effekt-ID zu GUID des Logs; Datensätze Effekt 45 und 51, Boden- und Agenten-Effekt), der an einem Spieler liegt oder von einem Spieler stammt. Effekte auf einer beweglichen Plattform (Flanking-Merker) fallen wie in C# weg; nur bei Logs mit Effektdaten |
| `EnemySrcEffectMechanic` | 13 | Effekt von einem Gegner oder, ohne Namen der Quelle, von der Umgebung ("Environment") |
| `PlayerSrcAllHealthDamageHitsMechanic` | 2 | Direkte Treffer eines Spielers mit beliebiger Fertigkeit; nur mit der Prüfung auf die Spezies des Ziels |
| `PlayerCastStartMechanic` | 8 | Beginn eines animierten Zaubers eines Spielers (Zauber mit höchstens 1 ms fallen weg; negative Ids sind Interaktionen und Gesten, keine Zauber) |

Gegner sind die Ziele der Logik und die Trash-Mobs ihrer Spezies-Liste (`GetTrashMobsIDs`, schon in `ei-data/targets`). Die animierten Zauber werden wie `CombatEventFactory.CreateCastEvents` gepaart: ein Beginn mit dem nächsten Ende derselben Fertigkeit (weicht die Dauer um mehr als 10 ms von der Zeit zwischen den Datensätzen ab, zählt diese), ein Beginn ohne Ende dauert die erwartete Zeit, höchstens bis zum Beginn des nächsten Zaubers des Wirkenden plus 10 ms, ein Ende ohne Beginn nur, wenn der Zauber vor dem Log begann. Nur Logs mit den Datensätzen für Animationsbeginn und -ende (arcdps ab 2019) sind ausgewertet.

### Lambda-Prüfungen bekannter Form

Lambdas lassen sich nicht per Reflexion lesen. `node tools/gen/Extract-Checkers.js` liest die Deklarationen im C#-Quelltext (Klammern und Zeichenketten werden ausgewertet, kein vollständiger Parser) und schreibt `docs/phase7/mechanic-filters.tsv`: für jede Mechanik, die außer einer Prüfung nichts anfügt, die Form der Prüfung, falls bekannt. `Gen-Mechanics.ps1` verbindet beides über Logikklasse, Typ und Kurzname und nimmt nur Prüfungen auf, die der Typ auswerten kann:

| Prüfung | Typ | Bedeutung |
| --- | --- | --- |
| `nostab` (33) | Treffer auf Spieler | der Spieler hat 10 ms vorher keine Stabilität (Buff 1122, Simulation von `ei-buffs`, nur wenn eine Mechanik sie braucht) |
| `damage` (5) | Treffer auf Spieler | Schaden über 0 (Direktschaden `value`, Buff-Schaden `buff_dmg`) |
| `killed`, `downed`, `downedorkilled` (15) | beliebiges Schadensereignis auf Spieler | Ergebnis 8 (Todesstoß) oder 9 (Niedergang); anders als bei Treffern zählt jedes Ereignis mit der Fertigkeit |
| `dur>=n`, `dur>n`, `dur<n`, `dur<=n` (32) | Zauberende, Zauberbeginn von Gegnern | tatsächliche Dauer des Zaubers |
| `interrupted`, `notinterrupted` (6) | Zauberbeginn und -ende | Endedatensatz mit Aktivierung 4 (Abbruch) |
| `speciesbuff` (13) | Buff-Anwendung auf Gegner | das Ziel ist von der Spezies und hat einen Buff (nicht), `ServerDelayConstant` vor dem Ereignis (`To.IsSpecies(X) && [!]To.HasBuff(log, Buff, Time[ - Delay])`): Febe (Verkörperungen mit und ohne Stärkung), Soo-Won. Der Buff kommt aus der Simulation von `ei-buffs` für den Gegner (die Simulation läuft je Gegner und Buff einmal). Die mit Cerus auftauchenden permanenten Verkörperungen sind in C# eine eigene Spezies und zählen nicht als die normale |
| `sub:…:stab`, `sub:…:nostab` (19 Untermechaniken, 24 mit den Wiederholungen je Trigger) | Untermechanik von `WithStabilitySubMechanic` | die Untermechanik hat die Ereignisse der Mutter, bei denen der Spieler 10 ms vorher Stabilität hat (`stab`) oder nicht hat (`nostab`); eine ignorierte Mutter (`UsingIgnored`) erscheint nicht selbst. Untermechaniken mit eigenem Lambda (Bee, Tank Eaten) sind nicht portiert |
| `buff:id,has|no,delay,window` (6) | Treffer auf Spieler | der Spieler hat den Buff (nicht) zum Zeitpunkt des Treffers, `delay` ms davor, bei `window` > 0 irgendwann innerhalb von ± `window` ms (`To.HasBuff(log, Buff, Time[, Window])`): Orbs des Festenkonstrukts, Decimas Fluxlanze und Funkenaura. Zeitpunkt: Segment mit Beginn ≤ Zeit < Ende, im Fenster jede Überschneidung |
| `species:to=…&direct` (1) | Treffer aller Fertigkeiten (`PlayerSrcAllHealthDamageHitsMechanic`) | nur Direktschaden eines Spielers selbst auf eine Spezies (Kern des Festenkonstrukts). Der Orb-Stoß der Drachenleere trifft einen Gadget mit eigener negativer Kennung, die Rust nicht kennt; er ist nicht portiert. Negative Spezies-Kennungen kennt Rust nur für die permanenten Verkörperungen von Cerus (−53 bis −58) |
| `applied=n` (3) | Buff auf Spieler | angewendete Dauer genau n ms |
| `species:to=…`, `species:by=…` (37, davon 36 ausgewertet) | Treffer, Buff-Anwendung, Schaden, Buff-Entfernen | Spezies des Ziels (`To.IsSpecies`) oder der Quelle (`By`, `From`, `CreditedBy`, `CreditedFrom`, `IsAnySpecies`): die Spezies-IDs stammen aus dem Enum `TargetID` (`SpeciesIDs.cs`), die Spezies eines Agenten aus der Agententabelle (NPC: Profession, Gadget: untere 16 Bit). Beim Entfernen aller Stapel ist die Quelle des Datensatzes das Ziel und das Ziel des Datensatzes die Quelle. Der Besitzer von Dienern (`GetFinalMaster`) ist nicht aufgelöst; in den 103 Logs ist das nicht nötig |

### Erfolgsbedingungen

Die 58 `AchievementEligibilityMechanic` (je Erfolg ein Paar `.L` = verloren und `.K` = behalten) beruhen auf `ComputeAchievementEligibilityEvents` der Logik. Portiert ist die Vorlage, der die meisten folgen (`InsertAchievementEligibityEventAndRemovePhase`, `AddSuccessBasedAchievementEligibityEvents`): je Spieler, der im Kampf bekannt ist, der erste Verlust (Treffer einer Fertigkeit, Anwendung eines Buffs) im Kampf, sonst am Kampfende "verloren", wenn der Kampf scheiterte, sonst "behalten". Die Regeln stehen in `RULES` (`mechanics.rs`): Seelenloser Schrecken, Löwenhof, Kosmisches Observatorium (Herausforderungsmodus), Geisterrennen, Flüsternder Schatten, Kaineng (Test der Reflexe und Größter Widerstand, beide Herausforderungsmodus; Letzterer zählt umgekehrt, nur im Erfolg), Greer (Herausforderungsmodus), Drachenleere und Xunlai-Schrottplatz (Klarheit; Unverschlungen im Herausforderungsmodus). Die Kampfart kommt aus `encounter::challenge_mode` (`GetLogMode` von Kaineng, Observatorium, Febe, Greer, Decima und Xunlai; die Story-Karte von Xunlai fehlt). Der Erfolg des Kampfes kommt aus `encounter::log_end_detail`. Es wird ein Kampf je Log angenommen (Instanz-Logs mit mehreren Kämpfen sind nicht ausgewertet).

Mit eigener Rechnung portiert: Febe (Unbegrenzter Optimismus: Verzweiflungstreffer, Tod oder Despawn, sonst Tod oder Abbruch irgendwo im Kampf = verloren zu Kampfbeginn) und Decima (Dieser Käfer kann tanzen: zuerst Treffer des Leuchtzauns, dann Exponiert-Buff innerhalb von 10 ms nach Fluxlanzen- oder Donnerschaden, dann Tod, Despawn, Tod oder Abbruch im Kampf; die Reihenfolge der Quellen entscheidet, nicht die Zeit). Tod und Despawn kommen wie bei `Dead` und `DC` aus den Statusdatensätzen der Spieler.

Nicht portiert, weil sie eigene Berechnungen haben (mehrere Kämpfe, Buffs): Qadim (Abwechselnd), Sabir, Ura, Wächterhain und Nexus der Ewigkeit.

Die Abklingzeit (ICD) gilt nur im HTML, nicht im JSON, und wird deshalb nicht angewendet. Die Spielerliste ist wie in C# auf aktive Spieler beschränkt.

Noch ohne Auswertung: `SubMechanic` mit eigenem Lambda (8), `EnemyStatus` (10), die übrigen Effekt-Typen (`EnemyDstEffect` und andere) und kleinere Typen. Mit anderem Zusatz (Lambda anderer Form, Untermechanik usw.) sind noch rund 400 Einträge aller Typen, darunter die Erfolgsbedingungen ohne Vorlage (siehe oben); sie brauchen je Mechanik Code.

## Messung (103 Korpus-Logs)

Encounter-Mechaniken auf Spielern aus Daten: 1.696 Vergleiche (je Mechanik sortierte Liste aus Zeit und Spieler), **keine Abweichung**.

Mechaniken auf Gegnern: 234 Vergleiche der Zeiten, **keine Abweichung**; bei den Grün-Effekten der Drachenleere (`S.Green`, 2 Logs) benennt C# die Quellen nach Drache und Himmelsrichtung ("Jormag Green E"), Rust nennt sie "Environment" (nur die Namen, `mechanics (enemy names)`). Die Namen der Gegner entsprechen denen von C# (`enemy_names` in `mechanics.rs`): Gleichnamige Ziele werden wie `NumericallyRenameSpecies` mit einer Zahl versehen; danach folgen die Umbenennungen der Logiken, die sich aus dem Log ergeben (Cerus: "Empowered …" bei dem Buff beim Auftauchen und "… (Permanent)" für Verkörperungen, die mit Cerus auftauchen und eine eigene Zählung haben; Deimos: Dieb, Trunkenbold, Spieler; Greer: "Godspoil Greer" mit Ereg; Ura: "Ura, the Steamshrieker" oder "Godscream Ura" nach den Lebenspunkten, vor arcdps 2026-09-29 "Elite/Champion/Legendary" für die Adds; Ai: "Elemental Ai, Keeper of the Peak") und zuletzt der Rang aus den Agenten-Informationen ("Veteran ", "Elite ", "Champion ", "Legendary "; arcdps ab 2026-09-29). Nicht portiert: die dunkle Ai, die Instanz Sunqua Peak, die Namen der übrigen Logiken (Splitter, Herzen, Phasen-Ziele, Xera und andere) und die Gemeinschaftsfarben der Trash-Mobs.

Abdeckung (`mechanics (not produced)`): 109 Paare aus Log und Mechanik, die C# mit Ereignissen meldet, erzeugt Rust noch nicht (vorher 264), darunter Untermechaniken mit Lambda, Effekt-Mechaniken mit Zusatz (Lambda, Lebensdauer), Spieler-Buff-Prüfungen, Quellen-Spezies mit Zusatz und Lambdas anderer Form.

Allgemeine Mechaniken:

14 Mechaniken je Log, 1.442 Vergleiche (sortierte Liste aus Zeit und Spieler), **3 Abweichungen**:

- **Lckt:** 3 Logs (Faultierion, Matthias zweimal): C# lässt einzelne Sperr-Ereignisse weg (bei Matthias genau die der geopferten Spieler, mit Quelle gleich Ziel und Dauer 10 s), die Regel dafür ist nicht gefunden. Quelle gleich Ziel allein ist es nicht (Fraenir hat solche Ereignisse, die zählen).

Die früheren DC-Abweichungen (Arkk, Kela, McLeod) waren keine künstlichen Ereignisse: es sind Despawn-Datensätze ohne Adresse, die C# über die Instanz-ID zuordnet.

Nicht nachgebildet: die Reihenfolge der Spielerliste von C# (nach Untergruppe, innerhalb der Gruppe nach Agentenreihenfolge, nicht nach Namen); sie wirkt nur bei Spielern mit nicht überlappenden Anwesenheitszeiten.

## Nicht portiert

- Die encounterspezifischen Mechaniken mit Zusatz und die Typen ohne Auswertung (siehe oben).
- Untermechaniken (Stabilität), Gewichte, interne Abklingzeiten, Zeitbegrenzer.
