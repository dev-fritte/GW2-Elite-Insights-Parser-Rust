# Phase 0.2: Reproduzierbarer C#-Build

- SDK ist über [`global.json`](../../global.json) auf 10.0.404 (`rollForward: latestPatch`) festgelegt.
- Build der kompletten Solution: `dotnet build GW2EI.slnx -c Release` (0 Warnungen, 0 Fehler, 51 s auf dem Messrechner).
- Ausgaben liegen in `GW2EI.bin/` (bereits per `.gitignore` ausgeschlossen).
- Hinweis: `GW2EIParserCLI.csproj` setzt `<Deterministic>false</Deterministic>`. Das betrifft nur Binär-Hashes, nicht die JSON-Ausgabe. Reproduzierbarkeit der Ausgabe wird in Schritt 0.3 geprüft.
