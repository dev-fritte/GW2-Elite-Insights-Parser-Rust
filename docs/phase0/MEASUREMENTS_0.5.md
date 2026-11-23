# Phase 0.5: C#-Schnelltests (Messdokument)

Zweck: messen, wie weit der bestehende C#-Code mit billigen Hebeln kommt, bevor über Rust entschieden wird (Go/No-Go in 0.6).
Alle Tests laufen über denselben Korpus (103 Logs, [corpus-manifest.tsv](corpus-manifest.tsv)) mit dem Messprogramm `GW2EIParserReference` (ein Log pro Prozess, alle Berechnungen, JSON serialisiert). Jeder Test wurde gegen die Referenz-Hashes geprüft ([reference-hashes.tsv](reference-hashes.tsv)): **bei jedem Test, der eine Ausgabe liefert, sind alle 103 JSON-Dateien byte-identisch.**
Rohdaten pro Log und Test: [results/](results/) (TSV). Verworfene Code-Experimente: [patches/](patches/).

Messrechner: Windows 11, 24 logische Kerne, 32 GB RAM, .NET SDK 10.0.404. Zeiten sind **Wall** (Prozess von außen gemessen, inklusive Runtime-Start und Laden der API-Daten), sofern nicht anders angegeben.

## Testliste

| Test | Änderung | Wie reproduziert | Parität | Ergebnis |
| --- | --- | --- | --- | --- |
| T0 | Baseline, Release, JIT-Standard, 3 Läufe | `Invoke-Corpus.ps1 -Label baseline-N` | 103/103 | Referenz |
| T0b | Baseline erneut, am Ende der Testreihe | `Invoke-Corpus.ps1 -Label base-recheck` | 103/103 | +4,4 % gegenüber T0: **Rauschband der Maschine ca. ±4 %** |
| T1 | `DOTNET_TieredPGO=0` (Dynamic PGO aus) | Umgebungsvariable, keine Codeänderung | 103/103 | **-18,6 %** |
| T2 | `DOTNET_TieredCompilation=0` | Umgebungsvariable | 103/103 | -2,6 %, Parse deutlich langsamer (749 auf 1.048 ms) |
| T3 | `DOTNET_gcConcurrent=0` | Umgebungsvariable | 103/103 | -2,4 %, Speicher-Spitze +24 % (304 auf 377 MB), im Rauschband |
| T4 | `DOTNET_GCgen0size=0x10000000` (256 MB) | Umgebungsvariable | 103/103 | +1,1 %, Speicher-Spitze +39 % (422 MB): **schadet** |
| T5 | T1 + T3 + T4 kombiniert | Umgebungsvariablen | 103/103 | -16,4 %, schlechter als T1 allein |
| T6 | Code: `BuffSimulatorIDIntensity.Update` ohne LINQ/`ToList`, Rekursion als Schleife | [patches/buffsim-intensity.patch](patches/buffsim-intensity.patch) | 103/103 | +0,2 %, Allokationen unverändert: **kein Gewinn, verworfen** |
| T7 | Code: 13 lineare Zeitfilter in `Actor.cs` durch Binärsuche ersetzt | [patches/binary-search-timefilter.patch](patches/binary-search-timefilter.patch) | 103/103 | -0,2 %, Allokationen -0,1 %: **kein Gewinn, verworfen** |
| T8 | ReadyToRun-Kompilierung (`PublishReadyToRun`, win-x64) | `dotnet publish -r win-x64 --self-contained false -p:PublishReadyToRun=true` | 103/103 | **-30,5 %** |
| T9 | T8 + `TieredPGO=0` | wie T8 plus Umgebungsvariable | 103/103 | **-36,8 %** |
| T10 | Multithread-Parse (`--threads`) | `Invoke-Corpus.ps1 -Threads` | nicht geprüft (nur Zeit) | -10,9 %; Parse langsamer, Build schneller |
| AOT | Native AOT (`PublishAot`) | `dotnet publish -r win-x64 -p:PublishAot=true` | - | **läuft nicht**, siehe unten |

T6 und T7 liefen im selben Zeitraum wie T0b; der Vergleich ist daher gegen T0b gezogen.

### Native AOT: nicht ohne Umbau lauffähig

Das Kompilieren gelingt (Linker braucht `vswhere.exe` im PATH), das Programm bricht aber beim Laden der API-Daten ab (`GW2SkillAPIController.GetAPISkills`). Ursache: Der Code nutzt reflexionsbasierte `System.Text.Json`-(De-)Serialisierung (viele Warnungen IL2026/IL3050 in `GW2EIGW2API`, `GW2EIDPSReport`). Auch mit `JsonSerializerIsReflectionEnabledByDefault=true` bleibt der Fehler. Native AOT würde Source-Generated-JSON und eine Trimming-Prüfung der gesamten Bibliothek erfordern. Das ist kein Schnelltest mehr.

### Profil (dotnet-trace, Greer-Log, 5,3 MB)

Das Sampling-Profil des größten Logs zeigt, wo die Zeit liegt: 64 % im Aufbau des JSON-Modells (`RawFormatBuilder`/`JsonLogBuilder`), davon ca. 36 % in den Spielern (`BuildJsonPlayer`) und 18 % in den NPCs; ca. 15 % Buff-Simulation; ca. 12 % Damage Modifier; 10 % JSON-Serialisierung. Querschnitt: ca. 41 % der Samples entfallen auf GC-Pausen, 12 % auf `Array.Copy`/`Array.Resize`, ca. 13 % auf `ToDictionary`, ca. 54 % inklusive unter `ToList`. Das erklärt, warum T6 und T7 nichts brachten: die linearen Filter und LINQ in der Buff-Simulation sind nicht der Engpass, die Last ist breit verteilt auf Allokation und Kopieren.

## Ergebnis pro Test (Mittel über 103 Logs)

| Test | Logs | Wall Summe s | Wall Median ms | Wall P90 ms | Total Summe s | Parse Median ms | Build Median ms | Peak Median MB | Alloc Summe GB | Wall vs T0b |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| T0 Baseline (Mittel aus 3) | 103 | 298,8 | 2.793 | 4.490 | 266,7 | 749 | 1.359 | 304 | 56,6 |  |
| T0b Baseline erneut | 103 | 311,9 | 2.891 | 4.749 | 278,9 | 771 | 1.381 | 304 | 56,6 | +0,0% |
| T1 TieredPGO aus | 103 | 253,9 | 2.388 | 3.719 | 222,2 | 693 | 1.062 | 300 | 56,6 | -18,6% |
| T2 TieredCompilation aus | 103 | 303,7 | 2.898 | 3.866 | 278,5 | 1.048 | 1.097 | 306 | 56,6 | -2,6% |
| T3 GC nicht-konkurrent | 103 | 304,5 | 2.800 | 4.611 | 271,5 | 726 | 1.421 | 377 | 56,6 | -2,4% |
| T4 Gen0-Budget 256 MB | 103 | 315,4 | 2.929 | 5.071 | 282,1 | 702 | 1.534 | 422 | 56,6 | +1,1% |
| T5 PGO aus + GC nicht-konkurrent + Gen0 256 MB | 103 | 260,7 | 2.398 | 3.834 | 227,9 | 687 | 1.081 | 440 | 56,6 | -16,4% |
| T6 Code: Buff-Intensity-Simulation | 103 | 312,5 | 2.888 | 4.690 | 279,4 | 772 | 1.388 | 304 | 56,6 | +0,2% |
| T7 Code: Binaersuche Zeitfilter | 103 | 311,4 | 2.887 | 4.725 | 278,5 | 764 | 1.417 | 304 | 56,6 | -0,2% |
| T8 ReadyToRun | 103 | 216,7 | 1.963 | 3.437 | 184,2 | 489 | 889 | 292 | 56,6 | -30,5% |
| T9 ReadyToRun + TieredPGO aus | 103 | 197,3 | 1.837 | 3.036 | 165,2 | 468 | 818 | 290 | 56,6 | -36,8% |
| T10 Multithread-Parse (CLI-Standard) | 103 | 278,0 | 2.598 | 4.114 | 245,8 | 1.094 | 859 | 310 | 56,8 | -10,9% |

Spalten: Wall/Total/Parse/Build in ms bzw. s (Summe über den Korpus), Peak = Spitzen-Arbeitsspeicher des Prozesses, Alloc = insgesamt allokierte Menge. "Wall vs T0b" vergleicht die Wall-Summe mit der Baseline-Wiederholung.

## CLI-Start (`GuildWars2EliteInsights-CLI.exe -v`, 30 Läufe)

| Variante | min ms | Median ms | max ms |
| --- | ---: | ---: | ---: |
| JIT (Standard) | 133 | 141 | 186 |
| JIT, TieredPGO aus | 129 | 131 | 137 |
| ReadyToRun | 96 | 100 | 174 |
| ReadyToRun, TieredPGO aus | 91 | 94 | 111 |

Zielwert aus dem Plan: unter 50 ms. Nicht erreicht. Hinzu kommt das Laden der API-JSONs (13 MB), median 216 bis 223 ms pro Prozess (Spalte Api in den Rohdaten); mit ReadyToRun kaum verändert (222 ms). Dieser Block wäre mit vorkompilierten Daten (`rkyv` in Rust, oder ein binäres Format in C#) der größte Hebel für kurze Läufe.

## Wall-Zeit pro Log und Test (ms)

Spalten T0 bis T10 wie in der Testliste. T0 ist der Mittelwert aus drei Läufen, alle anderen Einzelläufe. Eine Lücke "-" bedeutet nicht gemessen. Die Zeilen sind die Logs aus dem Korpus-Manifest (Ordner = Boss, Log = Dateiname im Korpus-Ordner).

| Ordner | Log | Bytes | T0 | T0b | T1 | T2 | T3 | T4 | T5 | T6 | T7 | T8 | T9 | T10 |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Albtraum-Oratuss (17028) | 20261104-212407.zevtc | 116.877 | 1.029 | 1.057 | 958 | 1.806 | 1.028 | 1.040 | 1.001 | 1.190 | 1.158 | 624 | 605 | 1.026 |
| Ankka (23957) | 20260719-230006.zevtc | 1.689.256 | 3.067 | 3.241 | 2.540 | 2.941 | 2.868 | 2.998 | 2.511 | 3.201 | 3.168 | 2.179 | 1.992 | 2.724 |
| Ankka (23957) | 20261022-210900.zevtc | 4.114.987 | 6.314 | 6.789 | 4.985 | 4.908 | 6.717 | 6.656 | 5.454 | 7.689 | 6.724 | 5.450 | 4.314 | 5.582 |
| Arkk (17759) | 20261104-211518.zevtc | 290.838 | 1.382 | 1.420 | 1.261 | 2.082 | 1.365 | 1.359 | 1.329 | 1.451 | 1.407 | 905 | 860 | 1.404 |
| Artsariiv (17949) | 20261104-211052.zevtc | 69.794 | 982 | 1.020 | 956 | 1.808 | 993 | 1.044 | 995 | 1.026 | 1.028 | 611 | 590 | 991 |
| �therische Barriere (47188) | 20260817-202125.zevtc | 650.991 | 1.385 | 1.419 | 1.301 | 2.029 | 1.327 | 1.402 | 1.322 | 1.428 | 1.431 | 894 | 861 | 1.381 |
| �therische Barriere (47188) | 20261109-202836.zevtc | 565.475 | 1.315 | 1.356 | 1.211 | 1.999 | 1.287 | 1.350 | 1.224 | 1.368 | 1.358 | 839 | 832 | 1.313 |
| Auge des Schicksals (19844) | 20261017-214727.zevtc | 937.611 | 1.694 | 1.752 | 1.510 | 2.218 | 1.617 | 1.694 | 1.520 | 1.765 | 1.756 | 1.124 | 1.081 | 1.640 |
| Auge des Schicksals (19844) | 20261024-000014.zevtc | 463.453 | 1.423 | 1.484 | 1.324 | 2.104 | 1.369 | 1.450 | 1.268 | 1.496 | 1.459 | 927 | 868 | 1.387 |
| Berg (16088) | 20260830-191503.zevtc | 3.068.590 | 2.793 | 2.898 | 2.482 | 2.898 | 2.719 | 3.006 | 2.413 | 2.936 | 2.901 | 1.987 | 1.842 | 2.677 |
| Berg (16088) | 20260907-205749.zevtc | 3.558.984 | 2.919 | 3.023 | 2.542 | 2.958 | 3.013 | 3.155 | 2.714 | 3.041 | 3.034 | 2.073 | 1.925 | 2.806 |
| Beschworene Verschmelzung (43974) | 20260901-222933.zevtc | 1.413.118 | 2.228 | 2.298 | 1.982 | 2.605 | 2.142 | 2.206 | 1.980 | 2.325 | 2.287 | 1.568 | 1.472 | 2.118 |
| Beschworene Verschmelzung (43974) | 20261019-002815.zevtc | 2.077.256 | 2.737 | 2.792 | 2.364 | 2.872 | 2.626 | 2.895 | 2.357 | 2.829 | 2.823 | 1.904 | 1.795 | 2.523 |
| Cairn der Unbeugsame (17194) | 20260706-220343.zevtc | 814.572 | 1.705 | 1.740 | 1.507 | 2.161 | 1.702 | 1.705 | 1.495 | 1.762 | 1.854 | 1.102 | 1.039 | 1.659 |
| Cairn der Unbeugsame (17194) | 20260826-201254.zevtc | 981.323 | 1.931 | 1.982 | 1.668 | 2.298 | 1.886 | 1.997 | 1.663 | 1.997 | 2.011 | 1.250 | 1.154 | 1.932 |
| Cerus (25989) | 20260827-214342.zevtc | 2.048.126 | 3.444 | 3.535 | 2.896 | 3.326 | 3.387 | 3.723 | 3.006 | 3.562 | 3.567 | 2.474 | 2.261 | 3.197 |
| Cerus (25989) | 20261104-231707.zevtc | 2.929.408 | 4.093 | 4.294 | 3.559 | 3.749 | 4.088 | 4.320 | 3.666 | 4.251 | 4.260 | 3.272 | 2.811 | 3.872 |
| Dagda (25705) | 20261105-223139.zevtc | 3.687.664 | 5.838 | 6.344 | 4.743 | 4.683 | 5.728 | 6.402 | 4.906 | 6.440 | 6.457 | 4.745 | 4.067 | 5.221 |
| Dagda (25705) | 20261115-231751.zevtc | 1.780.613 | 2.846 | 2.958 | 2.455 | 2.952 | 2.873 | 2.958 | 2.490 | 2.983 | 2.945 | 2.004 | 1.904 | 2.663 |
| Decima, die Sturms�ngerin (26774) | 20260923-223433.zevtc | 2.786.147 | 3.847 | 3.886 | 3.199 | 3.400 | 3.632 | 4.027 | 3.179 | 3.785 | 3.892 | 2.686 | 2.408 | 3.509 |
| Decima, die Sturms�ngerin (26774) | 20260930-222211.zevtc | 2.066.142 | 3.365 | 3.310 | 2.751 | 3.072 | 3.162 | 3.475 | 2.836 | 3.324 | 3.326 | 2.309 | 2.060 | 2.999 |
| Deimos (17154) | 20260826-210836.zevtc | 3.731.999 | 5.281 | 5.635 | 4.578 | 4.314 | 5.542 | 6.026 | 4.681 | 5.592 | 5.659 | 3.955 | 3.574 | 5.298 |
| Deimos (17154) | 20261102-220842.zevtc | 1.783.028 | 3.322 | 3.432 | 2.764 | 3.237 | 3.143 | 3.377 | 2.785 | 3.355 | 3.418 | 2.426 | 2.218 | 3.103 |
| Desmina (19828) | 20261018-233103.zevtc | 971.533 | 1.499 | 1.535 | 1.347 | 2.134 | 1.435 | 1.526 | 1.318 | 1.528 | 1.512 | 957 | 916 | 1.458 |
| Desmina (19828) | 20261110-235615.zevtc | 1.137.692 | 1.535 | 1.581 | 1.399 | 2.152 | 1.473 | 1.571 | 1.356 | 1.602 | 1.579 | 978 | 943 | 1.515 |
| Dhuum (19450) | 20260826-230424.zevtc | 1.703.603 | 2.449 | 2.535 | 2.114 | 2.711 | 2.487 | 2.431 | 2.204 | 2.514 | 2.528 | 1.686 | 1.615 | 2.383 |
| Dhuum (19450) | 20261019-233441.zevtc | 3.995.286 | 6.165 | 6.439 | 4.981 | 4.888 | 6.246 | 7.223 | 5.421 | 6.293 | 6.284 | 4.931 | 4.211 | 5.575 |
| Die Drachenleere (43488) | 20260823-212158.zevtc | 1.239.355 | 2.397 | 2.460 | 2.074 | 2.706 | 2.423 | 2.471 | 2.201 | 2.494 | 2.459 | 1.643 | 1.552 | 2.271 |
| Die Drachenleere (43488) | 20261118-233145.zevtc | 4.832.127 | 7.598 | 8.337 | 6.025 | 5.730 | 7.525 | 8.454 | 6.533 | 8.607 | 8.105 | 6.641 | 5.298 | 6.614 |
| Eisbrut-Konstrukt (22154) | 20260828-174316.zevtc | 639.856 | 1.699 | 1.745 | 1.517 | 2.251 | 1.668 | 1.706 | 1.577 | 1.756 | 1.756 | 1.097 | 1.035 | 1.655 |
| Eisbrut-Konstrukt (22154) | 20261022-202100.zevtc | 495.585 | 1.519 | 1.563 | 1.352 | 2.127 | 1.453 | 1.562 | 1.408 | 1.570 | 1.557 | 985 | 922 | 1.488 |
| Ensolyss der endlosen Pein (16948) | 20260824-232558.zevtc | 600.721 | 1.624 | 1.668 | 1.480 | 2.231 | 1.613 | 1.625 | 1.497 | 1.654 | 1.680 | 1.088 | 1.014 | 1.576 |
| Eparch (26231) | 20261104-204506.zevtc | 1.012.953 | 2.336 | 2.399 | 2.036 | 2.665 | 2.287 | 2.292 | 2.162 | 2.422 | 2.406 | 1.628 | 1.547 | 2.244 |
| Faultierion (16123) | 20260810-205841.zevtc | 1.424.241 | 2.607 | 2.665 | 2.234 | 2.753 | 2.691 | 2.705 | 2.332 | 2.696 | 2.672 | 1.838 | 1.734 | 2.417 |
| Faultierion (16123) | 20260928-204107.zevtc | 1.847.647 | 3.395 | 3.455 | 2.896 | 3.365 | 3.245 | 3.456 | 2.883 | 3.420 | 3.403 | 2.443 | 2.317 | 3.171 |
| Festenkonstrukt (16235) | 20260727-211545.zevtc | 2.122.666 | 3.421 | 3.520 | 2.934 | 3.216 | 3.481 | 3.618 | 3.010 | 3.548 | 3.516 | 2.549 | 2.336 | 3.229 |
| Festenkonstrukt (16235) | 20261109-220146.zevtc | 3.025.413 | 4.517 | 4.815 | 3.719 | 3.851 | 4.611 | 5.071 | 4.084 | 4.828 | 4.839 | 3.520 | 3.066 | 4.220 |
| Fl�sternder Schatten (27010) | 20261118-000706.zevtc | 277.171 | 1.360 | 1.413 | 1.230 | 2.060 | 1.367 | 1.345 | 1.299 | 1.391 | 1.406 | 851 | 854 | 1.326 |
| Fraenir Jormags (22492) | 20261019-223024.zevtc | 861.136 | 2.064 | 2.127 | 1.786 | 2.493 | 2.059 | 2.060 | 1.880 | 2.134 | 2.137 | 1.418 | 1.329 | 1.945 |
| Fraenir Jormags (22492) | 20261116-010716.zevtc | 738.719 | 1.905 | 1.966 | 1.646 | 2.356 | 1.866 | 1.861 | 1.724 | 1.956 | 1.949 | 1.273 | 1.188 | 1.807 |
| Fraenir of Jormag (22492) | 20261105-001404.zevtc | 785.002 | 2.015 | 2.061 | 1.723 | 2.415 | 1.992 | 1.959 | 1.800 | 2.052 | 2.047 | 1.336 | 1.263 | 1.904 |
| Gebrochener K�nig (19691) | 20260718-211212.zevtc | 754.616 | 1.546 | 1.557 | 1.350 | 2.095 | 1.510 | 1.549 | 1.326 | 1.584 | 1.580 | 983 | 927 | 1.497 |
| Gefl�ster des Jormag (22711) | 20261019-223833.zevtc | 1.220.501 | 2.361 | 2.441 | 2.016 | 2.642 | 2.397 | 2.371 | 2.124 | 2.422 | 2.415 | 1.632 | 1.513 | 2.250 |
| Gefl�ster des Jormag (22711) | 20261112-202523.zevtc | 1.048.892 | 2.214 | 2.265 | 1.864 | 2.490 | 2.176 | 2.224 | 1.930 | 2.286 | 2.258 | 1.475 | 1.370 | 2.068 |
| Gorseval der Facettenreiche (15429) | 20260928-202622.zevtc | 1.484.961 | 2.929 | 3.044 | 2.535 | 3.078 | 2.838 | 3.232 | 2.521 | 3.086 | 3.050 | 2.152 | 2.027 | 2.789 |
| Gorseval der Facettenreiche (15429) | 20261019-202452.zevtc | 1.219.901 | 2.421 | 2.496 | 2.098 | 2.719 | 2.352 | 2.408 | 2.173 | 2.504 | 2.504 | 1.673 | 1.573 | 2.273 |
| G�tterschauer-Decima (26867) | 20261018-003736.zevtc | 1.948.941 | 3.166 | 3.240 | 2.655 | 3.064 | 3.097 | 3.371 | 2.670 | 3.280 | 3.230 | 2.240 | 2.060 | 2.950 |
| G�tterschauer-Decima (26867) | 20261018-013712.zevtc | 4.132.872 | 5.236 | 5.486 | 4.538 | 4.545 | 5.465 | 5.870 | 4.680 | 5.612 | 5.500 | 4.306 | 3.924 | 4.882 |
| Gramvolle Zauberwirkerin (23254) | 20261104-205609.zevtc | 529.602 | 1.614 | 1.663 | 1.419 | 2.191 | 1.583 | 1.626 | 1.479 | 1.675 | 1.675 | 1.039 | 965 | 1.578 |
| Greer, der Pestilenzbringer (26725) | 20260809-230055.zevtc | 1.680.799 | 2.793 | 2.919 | 2.345 | 2.840 | 2.783 | 3.011 | 2.369 | 2.888 | 2.914 | 1.927 | 1.781 | 2.542 |
| Greer, der Pestilenzbringer (26725) | 20261017-033913.zevtc | 5.347.740 | 7.417 | 7.741 | 5.681 | 5.254 | 7.259 | 8.123 | 5.744 | 7.709 | 7.688 | 5.377 | 4.778 | 6.600 |
| Gro�er K�tzchen-Golem (19676) | 20261120-010937.zevtc | 100.848 | 1.071 | 1.111 | 985 | 1.751 | 1.059 | 1.121 | 1.011 | 1.105 | 1.103 | 668 | 631 | 1.062 |
| Kanaxai, Sense des Hauses Aurkus (25577) | 20261104-205113.zevtc | 638.641 | 2.293 | 2.342 | 1.956 | 2.642 | 2.200 | 2.319 | 1.960 | 2.369 | 2.353 | 1.613 | 1.503 | 2.229 |
| Kapit�n Mai Trin (24033) | 20260816-223956.zevtc | 2.641.500 | 5.326 | 5.846 | 4.214 | 4.294 | 5.282 | 5.801 | 4.237 | 5.794 | 5.776 | 4.101 | 3.572 | 4.585 |
| Kapit�n Mai Trin (24033) | 20261112-204348.zevtc | 1.528.183 | 3.484 | 3.749 | 2.840 | 3.256 | 3.402 | 3.569 | 2.923 | 3.652 | 3.710 | 2.565 | 2.273 | 3.072 |
| Kardinal Adina (22006) | 20260726-163254.zevtc | 1.940.763 | 2.896 | 2.971 | 2.570 | 2.928 | 2.945 | 3.155 | 2.525 | 3.011 | 2.970 | 2.100 | 1.949 | 2.774 |
| Kardinal Adina (22006) | 20260912-202018.zevtc | 731.227 | 1.737 | 1.797 | 1.578 | 2.334 | 1.754 | 1.737 | 1.569 | 1.778 | 1.770 | 1.134 | 1.070 | 1.704 |
| Kardinal Sabir (21964) | 20260706-225337.zevtc | 2.889.415 | 3.434 | 3.619 | 2.916 | 3.178 | 3.605 | 3.547 | 3.003 | 3.501 | 3.523 | 2.505 | 2.239 | 3.158 |
| Kardinal Sabir (21964) | 20260923-213051.zevtc | 1.996.259 | 2.759 | 2.835 | 2.363 | 2.879 | 2.966 | 2.885 | 2.542 | 2.826 | 2.839 | 1.963 | 1.802 | 2.598 |
| Kela, Seneschall der Wogen (27124) | 20260924-220937.zevtc | 4.012.090 | 5.667 | 6.188 | 4.750 | 4.736 | 5.995 | 6.209 | 4.986 | 6.112 | 6.054 | 4.550 | 3.980 | 5.120 |
| Kela, Seneschall der Wogen (27124) | 20261008-215831.zevtc | 1.977.976 | 3.139 | 3.217 | 2.664 | 3.133 | 3.168 | 3.362 | 2.673 | 3.203 | 3.199 | 2.233 | 2.082 | 2.924 |
| Knochenh�uter (22521) | 20261115-221842.zevtc | 879.071 | 1.783 | 1.794 | 1.535 | 2.278 | 1.730 | 1.733 | 1.570 | 1.797 | 1.804 | 1.134 | 1.073 | 1.675 |
| Knochenh�uter (22521) | 20261122-002632.zevtc | 1.189.651 | 1.891 | 1.958 | 1.645 | 2.328 | 1.891 | 1.880 | 1.637 | 1.936 | 1.931 | 1.225 | 1.167 | 1.829 |
| MAMA (17021) | 20260824-231300.zevtc | 193.515 | 1.253 | 1.271 | 1.154 | 1.971 | 1.282 | 1.221 | 1.189 | 1.283 | 1.287 | 781 | 767 | 1.222 |
| Matthias Gabrel (16115) | 20260706-210831.zevtc | 2.073.129 | 3.238 | 3.381 | 2.618 | 3.015 | 3.379 | 3.415 | 2.612 | 3.230 | 3.369 | 2.283 | 2.093 | 2.869 |
| Matthias Gabrel (16115) | 20261010-215533.zevtc | 1.434.544 | 2.598 | 2.612 | 2.170 | 2.771 | 2.800 | 2.620 | 2.268 | 2.623 | 2.621 | 1.759 | 1.640 | 2.391 |
| McLeod der Stille (16253) | 20260810-213038.zevtc | 3.398.660 | 3.534 | 3.733 | 3.008 | 3.316 | 3.841 | 3.664 | 3.121 | 3.846 | 3.503 | 2.561 | 2.405 | 3.189 |
| McLeod der Stille (16253) | 20261102-211741.zevtc | 2.454.906 | 3.159 | 3.240 | 2.704 | 3.128 | 3.394 | 3.204 | 2.656 | 3.236 | 3.222 | 2.310 | 2.129 | 2.921 |
| Minister Li (24266) | 20261118-221746.zevtc | 372.553 | 1.384 | 1.390 | 1.227 | 2.068 | 1.500 | 1.413 | 1.236 | 1.395 | 1.412 | 861 | 806 | 1.352 |
| Minister Li (24485) | 20260827-210937.zevtc | 2.496.957 | 3.341 | 3.460 | 2.867 | 3.246 | 3.868 | 3.756 | 2.841 | 3.445 | 3.413 | 2.461 | 2.246 | 3.043 |
| Minister Li (24485) | 20260908-011826.zevtc | 2.968.352 | 3.940 | 4.102 | 3.340 | 3.602 | 4.332 | 4.436 | 3.594 | 4.072 | 4.066 | 2.925 | 2.687 | 3.551 |
| Mursaat-Aufseher (17172) | 20260826-201928.zevtc | 1.375.461 | 2.628 | 2.649 | 2.162 | 2.680 | 2.622 | 2.791 | 2.151 | 2.634 | 2.653 | 1.843 | 1.616 | 2.495 |
| Mursaat-Aufseher (17172) | 20261008-224647.zevtc | 747.625 | 1.870 | 1.912 | 1.617 | 2.332 | 1.846 | 1.819 | 1.609 | 1.928 | 1.938 | 1.238 | 1.162 | 1.759 |
| Nikare (21105) | 20260823-183514.zevtc | 2.139.092 | 2.893 | 2.971 | 2.459 | 2.978 | 3.051 | 3.066 | 2.456 | 2.954 | 3.011 | 2.021 | 1.851 | 2.649 |
| Nikare (21105) | 20261003-172327.zevtc | 3.120.748 | 3.730 | 3.905 | 3.115 | 3.430 | 4.140 | 4.128 | 3.239 | 3.922 | 4.271 | 2.735 | 2.463 | 3.349 |
| Prototyp Vermillion (25413) | 20260709-212959.zevtc | 2.487.504 | 3.652 | 3.845 | 3.076 | 3.402 | 3.922 | 4.012 | 3.146 | 3.770 | 3.787 | 2.819 | 2.519 | 3.289 |
| Prototyp Vermillion (25413) | 20261112-211438.zevtc | 2.168.836 | 3.547 | 3.803 | 2.943 | 3.370 | 3.684 | 3.559 | 3.034 | 3.748 | 3.614 | 2.607 | 2.388 | 3.174 |
| Prototyp Vermillion (25414) | 20260719-224647.zevtc | 2.233.799 | 3.365 | 3.450 | 2.835 | 3.164 | 3.453 | 3.518 | 2.945 | 3.529 | 3.515 | 2.401 | 2.229 | 3.011 |
| Qadim (20934) | 20260810-173657.zevtc | 3.996.295 | 4.209 | 4.333 | 3.554 | 3.725 | 4.564 | 4.567 | 3.666 | 4.318 | 4.302 | 3.162 | 2.859 | 3.868 |
| Qadim (20934) | 20261013-202813.zevtc | 2.182.197 | 2.740 | 2.812 | 2.433 | 2.898 | 2.785 | 2.929 | 2.379 | 2.813 | 2.887 | 1.953 | 1.837 | 2.595 |
| Qadim der Unvergleichliche (22000) | 20260912-210524.zevtc | 2.184.335 | 3.227 | 3.306 | 2.713 | 3.146 | 3.314 | 3.276 | 2.809 | 3.299 | 3.264 | 2.294 | 2.188 | 2.945 |
| Qadim der Unvergleichliche (22000) | 20261010-204544.zevtc | 3.229.899 | 4.052 | 4.480 | 3.453 | 3.761 | 4.583 | 4.401 | 3.714 | 4.355 | 4.381 | 3.209 | 2.835 | 3.782 |
| Sabetha die Saboteurin (15375) | 20260817-204113.zevtc | 2.349.198 | 3.896 | 4.202 | 3.621 | 4.155 | 4.138 | 4.259 | 3.756 | 4.123 | 4.142 | 2.958 | 3.036 | 3.672 |
| Sabetha die Saboteurin (15375) | 20261005-203726.zevtc | 1.766.055 | 2.805 | 2.891 | 2.388 | 2.907 | 2.836 | 2.914 | 2.398 | 2.907 | 2.878 | 1.983 | 1.843 | 2.618 |
| Samarog (17188) | 20260726-223415.zevtc | 1.958.602 | 3.022 | 3.208 | 2.546 | 2.947 | 3.146 | 3.242 | 2.563 | 3.181 | 3.137 | 2.080 | 1.996 | 2.732 |
| Samarog (17188) | 20260826-203115.zevtc | 3.493.521 | 4.490 | 4.749 | 3.822 | 3.866 | 4.829 | 5.277 | 3.834 | 4.690 | 4.725 | 3.437 | 2.998 | 4.114 |
| Seelenloser Schrecken (19767) | 20261110-231208.zevtc | 1.667.724 | 2.678 | 2.830 | 2.244 | 2.827 | 2.777 | 2.822 | 2.303 | 2.853 | 2.837 | 1.819 | 1.668 | 2.601 |
| Seelenloser Schrecken (19767) | 20261110-235200.zevtc | 2.191.535 | 3.289 | 3.458 | 2.742 | 3.164 | 3.257 | 3.429 | 2.756 | 3.366 | 3.358 | 2.316 | 2.151 | 3.130 |
| Seelenverzehrer (19536) | 20261017-214230.zevtc | 933.171 | 1.623 | 1.682 | 1.447 | 2.190 | 1.610 | 1.657 | 1.465 | 1.658 | 1.666 | 1.039 | 1.002 | 1.569 |
| Skorvald der Zerschmetterte (17632) | 20261104-210914.zevtc | 345.622 | 1.485 | 1.525 | 1.370 | 2.159 | 1.504 | 1.490 | 1.381 | 1.516 | 1.544 | 975 | 932 | 1.451 |
| Spukende Statue (16247) | 20260713-214308.zevtc | 576.231 | 1.265 | 1.315 | 1.150 | 1.919 | 1.271 | 1.293 | 1.186 | 1.302 | 1.302 | 782 | 750 | 1.266 |
| Spukende Statue (16247) | 20261122-200656.zevtc | 704.422 | 1.396 | 1.424 | 1.288 | 2.091 | 1.441 | 1.376 | 1.256 | 1.444 | 1.438 | 894 | 880 | 1.362 |
| Standard-K�tzchen-Golem (16199) | 20261018-160536.zevtc | 89.371 | 1.033 | 1.069 | 962 | 1.708 | 1.064 | 1.064 | 981 | 1.065 | 1.059 | 632 | 622 | 1.026 |
| Standard-K�tzchen-Golem (16199) | 20261120-011447.zevtc | 302.163 | 1.219 | 1.249 | 1.102 | 1.853 | 1.240 | 1.229 | 1.111 | 1.253 | 1.249 | 752 | 722 | 1.214 |
| Stimme der Gefallenen (22343) | 20260817-224322.zevtc | 749.558 | 1.841 | 1.881 | 1.590 | 2.267 | 1.873 | 1.799 | 1.599 | 1.872 | 1.870 | 1.213 | 1.162 | 1.757 |
| Stimme der Gefallenen (22343) | 20260930-231025.zevtc | 529.192 | 1.666 | 1.709 | 1.467 | 2.234 | 1.680 | 1.686 | 1.511 | 1.741 | 1.748 | 1.081 | 1.003 | 1.599 |
| Tal-W�chter (15438) | 20260706-201850.zevtc | 1.317.858 | 2.560 | 2.640 | 2.188 | 2.719 | 2.661 | 2.649 | 2.306 | 2.692 | 2.671 | 1.818 | 1.675 | 2.360 |
| Tal-W�chter (15438) | 20260810-201656.zevtc | 1.699.990 | 2.926 | 2.968 | 2.498 | 2.958 | 3.023 | 3.008 | 2.519 | 2.992 | 2.976 | 2.076 | 1.924 | 2.566 |
| Ura (26712) | 20260829-153752.zevtc | 2.952.086 | 5.672 | 6.074 | 4.634 | 4.560 | 5.924 | 5.879 | 4.532 | 5.980 | 6.431 | 4.439 | 3.888 | 5.351 |
| Ura (26712) | 20261023-225312.zevtc | 1.900.185 | 4.205 | 4.517 | 3.514 | 3.743 | 4.268 | 4.568 | 3.680 | 4.435 | 4.390 | 3.253 | 2.851 | 3.926 |
| Vloxx (28106) | 20261015-215809.zevtc | 2.726.770 | 4.000 | 4.131 | 3.316 | 3.605 | 4.381 | 4.182 | 3.504 | 4.083 | 4.378 | 2.934 | 2.644 | 3.656 |
| Xera (16246) | 20260921-213025.zevtc | 1.749.259 | 3.030 | 3.150 | 2.588 | 3.005 | 3.230 | 3.172 | 2.664 | 3.221 | 3.201 | 2.178 | 2.006 | 2.822 |
| Xera (16246) | 20261109-223536.zevtc | 2.456.411 | 4.116 | 4.353 | 3.400 | 3.640 | 4.413 | 4.334 | 3.600 | 4.316 | 4.305 | 3.042 | 2.850 | 3.808 |

## Fazit für die Entscheidung in 0.6

1. **Ohne Codeänderung** sind mit ReadyToRun und ausgeschaltetem Dynamic PGO **37 % kürzere Wall-Zeit** erreichbar (Summe 312 auf 197 s, Median 2,9 auf 1,8 s, P90 4,7 auf 3,0 s). Das sind zwei Build-Einstellungen (`PublishReadyToRun`, `TieredPGO=false` in der `runtimeconfig`), mit identischer Ausgabe auf allen 103 Logs.
2. **Die beiden getesteten Code-Hebel bringen nichts Messbares**: Binärsuche für Zeitfilter (T7) und eine allokationsärmere Buff-Intensity-Simulation (T6). Die Annahme der statischen Analyse aus dem Plan, dass diese Stellen der Engpass sind, ist damit für diesen Korpus **nicht bestätigt**. Das Profil zeigt eine breite Last aus Allokation, Kopieren und GC.
3. **Der Zielwert "typischer Log unter 1 s" wird mit C# billig nicht erreicht**: Median 1,8 s Wall, davon rund 0,22 s API-Daten und ca. 0,1 s Runtime-Start. Reiner Parse+Build-Median liegt bei 1,3 s. Ob Rust das Ziel erreicht, ist offen, die Daten sprechen aber dafür, dass ein Spalten-Layout und weniger Allokation (statt algorithmischer Tricks) der relevante Hebel wären.
4. **Speicher**: Keiner der Tests senkt den Spitzen-Speicher wesentlich (Median 304 auf 290 MB bei ReadyToRun). Der Zielwert "Faktor 3 bis 5 niedriger" ist mit C#-Konfiguration nicht erreichbar.
5. **Native AOT ist ohne größeren Umbau nicht möglich** (reflexionsbasiertes JSON).
6. Die Abbruchregel des Plans (nach 0.5 nur weitermachen, wenn C# allein die Ziele nicht erreicht) ist teilweise erfüllt: Laufzeit-Ziel verfehlt, Speicher-Ziel verfehlt, aber der Abstand zum Ziel ist bei der Laufzeit von 2,9x auf 1,8x geschrumpft. Das ist die Grundlage für 0.6.
