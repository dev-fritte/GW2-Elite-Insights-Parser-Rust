# Phase 1.0: Orakel für den Rust-Ingest

Das Orakel reduziert ein Log mit den **originalen C#-Lesefunktionen** von `EvtcParser` auf Zähler, Skalare und SHA256-Hashes über ein festes Binärlayout. Der Rust-Crate `evtc-ingest` (Schritte 1.2 und 1.3) muss dieselben Werte berechnen.

Werkzeug: `GW2EIParserReference --log <datei> --oracle <dump.json>` (Quelle: `GW2EI.Executables/GW2EI.Tests/GW2EIParserReference/OracleDump.cs`).
Die privaten Methoden `ParseLogData`, `ParseAgentData`, `ParseSkillData`, `ReadCombatItem[Rev1]` und `ParseCombatList` werden per Reflection aufgerufen, die Bibliothek bleibt unverändert.
Treiber über den Korpus: `tools/phase0/Invoke-Oracle.ps1`. Ergebnis im Repo: [`oracle-hashes.tsv`](oracle-hashes.tsv).
Die vollständigen Dumps (mit Agentennamen, also Spieler- und Account-Namen) liegen nur lokal und gehören nicht ins Repo.

## Zwei Schichten

| Schicht | Inhalt | Gehört zu |
| --- | --- | --- |
| **A: Rohdaten** | Alles, was in der Datei steht: Header, Agententabelle, Skilltabelle, alle 64-Byte-Ereignisse, ohne Filter, Zeiten unverändert | Gate 1.2 und 1.3 |
| **B: Behalten** | Was `ParseCombatList` übrig lässt (Gültigkeitsfilter `IsValid`, Zeit-Offset, Abschneiden nach Log-Ende) | Optional in 1.3, spätestens Phase 3 |

Die Trennung ist nötig, weil `ParseCombatList` neben dem Lesen schon fachliche Logik enthält (Filter, Karten-ID, Trigger-Korrektur, GW2-Build). Ein reiner Ingest-Crate soll nur Schicht A liefern müssen.

## Schicht A: kanonisches Layout

Alle Zahlen little endian. Hashes sind SHA256, in Großbuchstaben-Hex.

**Header** (16 Byte): 12 Byte ASCII `EVTC<yyyymmdd>`, 1 Byte Revision, 2 Byte Trigger-ID (u16), 1 Byte ignoriert. Ausgabe: `build_string`, `evtc_version` (Zahl hinter `EVTC`), `revision`, `trigger_id`.

**Agententabelle**: `u32` Anzahl, dann je 96 Byte. Hash über je Agent:

```
u64 agent_address
u32 prof (roh)        u32 is_elite (roh)
u16 toughness   u16 concentration   u16 healing   u16 condition
u32 hitbox_width (2 * roher u16)    u32 hitbox_height (2 * roher u16)
u32 name_length (Bytes)             name (UTF-8 Bytes)
```

Der Name ist der **volle 68-Byte-Block, nicht bei NUL abgeschnitten** (`GetString(reader, 68, false)`), UTF-8 mit Ersetzungszeichen für ungültige Bytes (`Encoding.UTF8.GetString`). Er enthält also Trailing-NULs und bei Spielern `Charakter\0Account\0Gruppe`.
Feldreihenfolge in der Datei: Adresse, prof, is_elite, toughness, **concentration**, healing, hitbox_width, condition, hitbox_height, Name. Das Feld heißt im Kommentar des C#-Codes "healing", ist aber `concentration`; maßgeblich ist die Reihenfolge im Quelltext.

**Skilltabelle**: `u32` Anzahl, dann je 68 Byte (`i32` ID, 64 Byte Name). Hash über je **eindeutige** Skill-ID in der Reihenfolge des ersten Auftretens (`TryAdd`, spätere Duplikate werden verworfen):

```
i32 skill_id   u32 name_length   name (UTF-8 Bytes, am ersten NUL abgeschnitten)
```

Der Name wird hier bei NUL abgeschnitten, anders als bei Agenten.

**Ereignisse**: Der Rest der Datei in 64-Byte-Records, Anzahl = `Restbytes / 64`. Restbytes ohne vollen Record werden als `trailing_bytes` gemeldet und ignoriert. Hash über je Record in Dateireihenfolge, jeweils die **64 Byte kanonisch**:

```
i64 time   u64 src_agent   u64 dst_agent   i32 value   i32 buff_dmg   u32 overstack_value   u32 skill_id
u16 src_instid   u16 dst_instid   u16 src_master_instid   u16 dst_master_instid
u8 iff   u8 buff   u8 result   u8 is_activation   u8 is_buffremove   u8 is_ninety   u8 is_fifty   u8 is_moving
u8 is_statechange   u8 is_flanking   u8 is_shields   u8 is_offcycle   u32 pad
```

Für Revision 1 entspricht das exakt den Rohbytes, **mit einer Ausnahme**: `is_statechange` wird wie in C# abgebildet (`GetStateChange`): Werte ab 93 (`StateChange.Unknown`) werden auf 93 gesetzt. Der Rust-Crate muss dieselbe Klemmung anwenden, sonst weichen Hashes bei unbekannten State-Changes ab.
Für Revision 0 (Header-Byte 12 = 0) liest C# ein anderes Layout (u16 `skill_id`, u16 `overstack`, 9 Byte Müll, `dst_master_instid` und `pad` gleich 0). Die Hashes werden aus den **dekodierten** Feldern gebildet, nicht aus Rohbytes.

Zusätzlich im lokalen Dump: Histogramm der State-Changes, erste und letzte drei Ereignisse (Feldwerte und Hex), `first_time`/`last_time`. Hinweis: `first_time`/`last_time` sind nicht aussagekräftig, weil bei Nicht-Zeit-Ereignissen das Feld andere Daten trägt.

## Schicht B: was `ParseCombatList` behält

Ausgabe: `kept.count`, `kept.hash` (gleiches 64-Byte-Layout, aber nach dem Zeit-Offset), `kept.trigger_id` (nach Instanz-Korrektur), `map_id`, `log_start_offset`, `log_end_time`, `gw2_build`. Schlägt `ParseCombatList` fehl (zu kurz, zu lang, keine Ereignisse), steht der Fehler in `kept_error`.

## Gate für 1.2 und 1.3

Für alle Logs im Manifest müssen in `oracle-hashes.tsv` übereinstimmen: `BuildString`, `Revision`, `TriggerId`, `Agents`, `AgentsHash`, `SkillsRaw`, `SkillsUnique`, `SkillsHash`, `Events`, `TrailingBytes`, `EventsHash`. Die Spalten ab `KeptCount` sind Schicht B.

## Grenzen

- Alle Korpus-Logs sind aus dem Zeitraum Juli bis November 2026. Revision 0 und alte Header-Versionen sind nicht abgedeckt; der Rev-0-Pfad ist nur über die Beschreibung oben spezifiziert und nicht per Hash geprüft.
- Beim Agenten-Hash kommen `prof` und `is_elite` direkt aus den Rohbytes (C# wandelt sie in `Spec` um), alle anderen Felder aus dem C#-`AgentItem`.

## Ergebnis (Stand 2026-11-23)

| Prüfung | Ergebnis |
| --- | --- |
| Logs im Orakel | 103/103 ohne Fehler |
| Zwei Läufe | Hash-Tabelle byte-identisch; auch alle 103 vollständigen Dumps identisch |
| Revision | alle Logs Revision 1 (Revision 0 nicht im Korpus) |
| arcdps-Builds | 10 verschiedene, `EVTC20260705` bis `EVTC20261115` |
| Rest-Bytes nach dem letzten Record | in keinem Log |
| Fehler in `ParseCombatList` | in keinem Log |

## Korrektur nach dem ersten Rust-Vergleich (1.2)

Der erste Rust-Vergleich zeigte in allen Logs einen abweichenden `AgentsHash`. Ursache war das Orakel selbst: Der C#-Konstruktor von `AgentItem` überschreibt bei Spielern die Hitbox mit festen Werten (48 x 240). Das Orakel hat diese verarbeiteten Werte gehasht, obwohl diese Spezifikation die doppelten Rohwerte verlangt. Das Orakel liest die Hitbox-Felder jetzt direkt aus den Rohbytes. Die Tabelle wurde neu erzeugt, zwei Läufe sind byte-identisch.

## Ergebnis des Rust-Vergleichs (1.2 und 1.3)

`evtc-oracle` (Rust) gegen diese Tabelle, 103 Logs: **0 Abweichungen in 11 Spalten** (`BuildString`, `Revision`, `TriggerId`, `Agents`, `AgentsHash`, `SkillsRaw`, `SkillsUnique`, `SkillsHash`, `Events`, `TrailingBytes`, `EventsHash`).

Aufruf:

```powershell
cargo run --release -p evtc-oracle -- --corpus <korpus-ordner> --manifest ../docs/phase0/corpus-manifest.tsv --oracle ../docs/phase1/oracle-hashes.tsv
```

## Ergebnis für Schicht B (Phase 3, Port von `ParseCombatList`)

`ei_core::keep::keep_events` (Rust) gegen die Spalten ab `KeptCount`, 103 Logs: **0 Abweichungen in 19 Spalten** (Schicht A und B zusammen: `KeptCount`, `KeptHash`, `KeptTriggerId`, `MapId`, `LogStartOffset`, `LogEndTime`, `Gw2Build`, `KeptError`).
Damit sind Gültigkeitsfilter, Karten- und Instanzregeln, Zeit-Normalisierung und die Erkennung der Healing-Stats-Extension bit-genau auf diesem Korpus bestätigt.
