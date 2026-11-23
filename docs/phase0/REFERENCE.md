# Phase 0.3: Referenz-JSON

Werkzeuge: `GW2EIParserReference` (Parse + `RawFormatBuilder` + JSON, ein Log pro Prozess), `tools/phase0/Invoke-Corpus.ps1`, `tools/phase0/Compare-Results.ps1`.
Einstellungen: wie die CLI mit allen Berechnungen (Cast, Replay, Mechaniken, Phasen, Extensions, Damage Modifiers, Buffs, Damage), `DetailedWvWParse`, Parser-Version 1.0, Parsing ohne Multithread-Beschleunigung.

Die Referenz-JSONs (je 0,5 bis 40 MB, mit Spielernamen) liegen lokal in `D:\_Projekte\GW2EI-Phase0\refB1` und `refB2`, nicht im Repo.
Im Repo liegen nur die SHA256-Summen: [`reference-hashes.tsv`](reference-hashes.tsv).

## Befund: Referenz war zunächst nicht reproduzierbar

Beim ersten Doppellauf waren 17 von 103 JSON-Dateien verschieden (86 gleich). Ursache: `AgentData.AddCustomNPCAgent` und `AddCustomAgentFrom` würfelten Agent-Adresse und `instanceID` künstlicher Agenten mit einem ungeseedeten `new Random()`. Betroffene Encounter: Berg/Bandit-Trio, Conjured Amalgamate, Desmina/River, Harvest Temple, Slothasor, Broken King, Matthias, Haunting Statue/Twisted Castle, Xera.

Behebung: fester Seed (siehe Commit "seed Random for custom agent ids"). Das ist die einzige Abweichung vom Upstream-Verhalten; sie betrifft nur die zufälligen IDs synthetischer Agenten, die ohnehin nicht stabil waren. Die 86 vorher stabilen Logs liefern byte-identisches JSON zu vorher.

## Gate

| Prüfung | Ergebnis |
| --- | --- |
| Zwei Läufe über alle 103 Korpus-Logs | 103/103 byte-identisches JSON (SHA256) |
| Vorher stabile Logs nach dem Fix unverändert | 86/86 identisch |
| Fehlgeschlagene Logs | 0 |

Hinweis: Bei Upstream-Patches mit neuen Encountern muss das Manifest und diese Hash-Liste neu erzeugt werden.
