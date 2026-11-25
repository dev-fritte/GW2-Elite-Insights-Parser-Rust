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
| `DC`, `Resp` | Despawn und Spawn der Spieler |
| `Res` | Beginn der Animation des Wiederbelebens (Skill 1066); eine Animation, die in derselben Millisekunde endet, zählt nicht |
| `Knck.Dwn`, `Knck.Pll`, `Flt`, `Lnch`, `Lckt`, `Wtr.Flt.Snk` | Crowd Control (Ergebnis 12) auf Spieler, nach dem generischen arcdps-Skill |

Zeiten sind relativ zum Kampfbeginn, nur Ereignisse im Kampf zählen.

## Messung (103 Korpus-Logs)

12 Mechaniken je Log, 1.236 Vergleiche (sortierte Liste aus Zeit und Spieler), **16 Abweichungen**:

- **Bekannte Ursachen anderswo:** Deimos (Kampfende), Drachenleere (Kampfende), Seelenverzehrer (Kampfbeginn 1 s) mit zusammen 8.
- **DC:** 3 Logs (Arkk, Kela, McLeod): C# erzeugt dort zusätzliche, künstliche Despawn-Ereignisse in der Statusverwaltung der Spieler (`PlayerActorStatusHelper`), die der Core nicht nachbildet.
- **Lckt:** 3 Logs (Faultierion, Matthias zweimal): C# lässt einzelne Sperr-Ereignisse weg (bei Matthias genau die der geopferten Spieler, mit Quelle gleich Ziel und Dauer 10 s), die Regel dafür ist nicht gefunden. Quelle gleich Ziel allein ist es nicht (Fraenir hat solche Ereignisse, die zählen).

## Nicht portiert

- `Miss.Qck` und `Miss.Alac` (`PlayerMissedBuffApplyEvent`, prüft die Subgruppen-Anbieter) kommen in fast jedem Log vor und brauchen die Buff-Verläufe der Gruppe.
- Alle encounterspezifischen Mechaniken (rund 1.100 Definitionen in 87 Logiken).
- Untermechaniken (Stabilität), Gewichte, interne Abklingzeiten, Zeitbegrenzer.
