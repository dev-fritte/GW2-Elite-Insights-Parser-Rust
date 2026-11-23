# Phase 0.4: Baseline (C#-Referenz, Release, JIT-Standard)

Messrechner: Windows 11, 24 logische Kerne, 32 GB RAM, .NET SDK 10.0.404, Commit-Stand vor den Optimierungen aus 0.5 (nach dem Determinismus-Fix).
Programm: `GW2EIParserReference` (Release), ein Log pro Prozess, alle Berechnungen an, JSON in den Speicher serialisiert.
Korpus: 103 Logs laut [corpus-manifest.tsv](corpus-manifest.tsv). Rohdaten lokal in `D:\_Projekte\GW2EI-Phase0\results`.

Begriffe: **Parse** = `EvtcParser.ParseLog`, **Build** = `RawFormatBuilder` (hier läuft die gesamte Auswertung), **Json** = Serialisierung, **Total** = Summe der drei, **Wall** = Prozess von außen gemessen (inkl. Runtime-Start, API-Daten laden, Datei schreiben), **Peak** = Prozess-Spitzen-Arbeitsspeicher, **Api** = Laden der API-JSONs. Zeiten in ms, Speicher in MB.

## Ergebnis: Standard (ohne Multithread-Beschleunigung), Mittel aus 3 Läufen

Konfiguration: baseline-1, baseline-2, baseline-3 (103 Logs, Mittel ueber 3 Lauf/Laeufe)

| Messgroesse | Summe | Median | P90 | Max | Mittel |
| --- | ---: | ---: | ---: | ---: | ---: |
| Wall | 298.798 | 2.793 | 4.490 | 7.598 | 2.901 |
| Total | 266.693 | 2.487 | 4.168 | 7.232 | 2.589 |
| Parse | 80.483 | 749 | 1.149 | 1.970 | 781 |
| Build | 152.259 | 1.359 | 2.379 | 4.879 | 1.478 |
| Json | 33.951 | 300 | 528 | 914 | 330 |
| Alloc | 57.978 | 528 | 1.052 | 1.889 | 563 |
| Peak | 33.122 | 304 | 524 | 878 | 322 |
| Api | 22.338 | 216 | 220 | 227 | 217 |

| Log | Datei | Bytes | Total ms | Parse ms | Build ms | Json ms | Peak MB |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Die Drachenleere (43488) | 20261118-233145.zevtc | 4.832.127 | 7.232 | 1.970 | 4.348 | 914 | 878 |
| Greer, der Pestilenzbringer (26725) | 20261017-033913.zevtc | 5.347.740 | 7.076 | 1.493 | 4.879 | 703 | 697 |
| Ankka (23957) | 20261022-210900.zevtc | 4.114.987 | 5.957 | 1.193 | 3.891 | 872 | 841 |
| Dhuum (19450) | 20261019-233441.zevtc | 3.995.286 | 5.811 | 1.124 | 3.793 | 894 | 803 |
| Dagda (25705) | 20261105-223139.zevtc | 3.687.664 | 5.501 | 1.149 | 3.709 | 644 | 650 |
| Ura (26712) | 20260829-153752.zevtc | 2.952.086 | 5.328 | 1.133 | 3.597 | 599 | 664 |
| Kela, Seneschall der Wogen (27124) | 20260924-220937.zevtc | 4.012.090 | 5.323 | 1.444 | 3.127 | 752 | 662 |
| Kapit�n Mai Trin (24033) | 20260816-223956.zevtc | 2.641.500 | 4.980 | 892 | 3.429 | 659 | 635 |

Streuung: Die drei Läufe unterscheiden sich in der Summe um unter 0,5 %; die JSON-Ausgabe ist in allen drei Läufen byte-identisch zur Referenz (103/103).

## Variante mit Multithread-Beschleunigung (`--threads`, entspricht CLI-Standard ohne `SingleThreaded`), 1 Lauf

Konfiguration: baseline-threads (103 Logs, Mittel ueber 1 Lauf/Laeufe)

| Messgroesse | Summe | Median | P90 | Max | Mittel |
| --- | ---: | ---: | ---: | ---: | ---: |
| Wall | 278.025 | 2.598 | 4.114 | 6.614 | 2.699 |
| Total | 245.847 | 2.286 | 3.785 | 6.259 | 2.387 |
| Parse | 116.109 | 1.094 | 1.693 | 2.634 | 1.127 |
| Build | 96.125 | 859 | 1.522 | 3.076 | 933 |
| Json | 33.613 | 297 | 514 | 905 | 326 |
| Alloc | 58.143 | 530 | 1.054 | 1.893 | 564 |
| Peak | 33.715 | 310 | 525 | 887 | 327 |
| Api | 22.317 | 216 | 219 | 230 | 217 |

## CLI-Start

`GuildWars2EliteInsights-CLI.exe -v` (Release, 20 Läufe): min 130 ms, Median 132 ms, max 139 ms. Zielwert aus dem Plan: unter 50 ms.
Das Laden der vier API-JSONs (13 MB) kostet zusätzlich im Median 216 ms pro Prozess (Spalte Api).

## Befunde

1. **Der Zielwert "typischer Raid-Log unter 1 s" ist weit entfernt:** Median Total 2,5 s, Median Wall 2,8 s, P90 4,5 s, Maximum 7,6 s.
2. **Build dominiert**: 57 % der Total-Zeit (Auswertung), Parse 30 %, Json 13 %. Das bestätigt die Annahme des Plans, dass die Laufzeit in der Auswertung entsteht, nicht beim Einlesen.
3. **Speicher:** Median 304 MB, Maximum 878 MB Spitzen-Arbeitsspeicher. Es werden im Median 528 MB allokiert (Maximum 1,9 GB), also ungefähr das 1,7-Fache des Spitzen-Speichers.
4. **Multithread-Beschleunigung** bringt nur etwa 8 % bei Total: Parse wird langsamer (749 auf 1.094 ms), Build schneller (1.359 auf 859 ms).
5. **Feste Kosten pro Prozess:** rund 130 ms Start und 216 ms API-Daten, zusammen ungefähr 350 ms. Bei einem 1-s-Ziel ist das ein Drittel des Budgets.
