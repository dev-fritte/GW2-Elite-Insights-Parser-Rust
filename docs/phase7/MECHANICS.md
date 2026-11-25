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

Die Abklingzeit (ICD) gilt nur im HTML, nicht im JSON, und wird deshalb nicht angewendet. Die Spielerliste ist wie in C# auf aktive Spieler beschränkt.

Noch ohne Auswertung, aber schon in der TSV und ohne Zusatz: `EnemyCastStart` (91), `EnemyDstBuffApply` (84), `EnemyCastEnd` (5), `PlayerSrcHit` (11), `PlayerCastStart` (10), `SpawnMechanic` (18), `SubMechanic` (27). Mit Zusatz (Lambda usw.) sind gut 500 Einträge aller Typen; sie brauchen je Mechanik Code.

## Messung (103 Korpus-Logs)

Encounter-Mechaniken aus Daten: 1.190 Vergleiche (je Mechanik sortierte Liste aus Zeit und Spieler), **9 Abweichungen**, alle aus den bekannten Zeitursachen (Deimos Kampfende 6, Seelenverzehrer Kampfbeginn 3).

Allgemeine Mechaniken:

14 Mechaniken je Log, 1.442 Vergleiche (sortierte Liste aus Zeit und Spieler), **17 Abweichungen**:

- **Bekannte Ursachen anderswo:** Deimos (Kampfende), Drachenleere (Kampfende) und Seelenverzehrer (Kampfbeginn 1 s) mit zusammen 14.
- **Lckt:** 3 Logs (Faultierion, Matthias zweimal): C# lässt einzelne Sperr-Ereignisse weg (bei Matthias genau die der geopferten Spieler, mit Quelle gleich Ziel und Dauer 10 s), die Regel dafür ist nicht gefunden. Quelle gleich Ziel allein ist es nicht (Fraenir hat solche Ereignisse, die zählen).

Die früheren DC-Abweichungen (Arkk, Kela, McLeod) waren keine künstlichen Ereignisse: es sind Despawn-Datensätze ohne Adresse, die C# über die Instanz-ID zuordnet.

Nicht nachgebildet: die Reihenfolge der Spielerliste von C# (nach Untergruppe, innerhalb der Gruppe nach Agentenreihenfolge, nicht nach Namen); sie wirkt nur bei Spielern mit nicht überlappenden Anwesenheitszeiten.

## Nicht portiert

- Die encounterspezifischen Mechaniken mit Zusatz und die Typen ohne Auswertung (siehe oben).
- Untermechaniken (Stabilität), Gewichte, interne Abklingzeiten, Zeitbegrenzer.
