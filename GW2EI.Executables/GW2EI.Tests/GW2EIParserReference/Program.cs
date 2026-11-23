using System.Diagnostics;
using System.Globalization;
using System.Security.Cryptography;
using System.Text.Json;
using GW2EIBuilders;
using GW2EIEvtcParser;
using GW2EIGW2API;

namespace GW2EIParserReference;

/// <summary>
/// Parst genau ein Log mit den Einstellungen der CLI (alle Berechnungen an) und schreibt optional das Referenz-JSON.
/// Gibt eine Zeile "RESULT {json}" mit Zeiten, Allokationen und Spitzen-Speicher aus.
/// Aufruf: GW2EIParserReference --log &lt;datei&gt; [--out &lt;json&gt;] [--threads] [--runs N] [--no-json]
/// </summary>
internal static class Program
{
    private sealed class Controller : ParserController
    {
        public override void UpdateProgressWithCancellationCheck(string status) { }
    }

    private static int Main(string[] args)
    {
        CultureInfo.CurrentCulture = CultureInfo.DefaultThreadCurrentCulture = CultureInfo.CreateSpecificCulture("en-US");
        string? logPath = null, outPath = null;
        string? oraclePath = null;
        bool threads = false, noJson = false;
        int runs = 1;
        for (int i = 0; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--log": logPath = args[++i]; break;
                case "--out": outPath = args[++i]; break;
                case "--threads": threads = true; break;
                case "--no-json": noJson = true; break;
                case "--oracle": oraclePath = args[++i]; break;
                case "--runs": runs = int.Parse(args[++i], CultureInfo.InvariantCulture); break;
                default: Console.Error.WriteLine($"Unbekanntes Argument {args[i]}"); return 2;
            }
        }
        if (logPath == null) { Console.Error.WriteLine("--log fehlt"); return 2; }

        var processStart = Process.GetCurrentProcess().StartTime;
        var sinceStartMs = (DateTime.Now - processStart).TotalMilliseconds;

        var content = Path.Combine(AppContext.BaseDirectory, "Content");
        var sw = Stopwatch.StartNew();
        var api = new GW2APIController(
            Path.Combine(content, "SkillList.json"), Path.Combine(content, "SpecList.json"),
            Path.Combine(content, "TraitList.json"), Path.Combine(content, "MapList.json"));
        var apiMs = sw.Elapsed.TotalMilliseconds;

        var settings = new EvtcParserSettings(2200, 150)
        {
            ComputeCast = true,
            ComputeCombatReplay = true,
            ComputeMechanics = true,
            ComputePhases = true,
            DetailedWvWParse = true,
            ParseExtensions = true,
            ComputeDamageModifiers = true,
            ComputeBuff = true,
            ComputeDamage = true,
        };

        if (oraclePath != null)
        {
            var (compact, full) = OracleDump.Run(logPath, new EvtcParser(settings, api));
            if (oraclePath != "-") { File.WriteAllText(oraclePath, full); }
            Console.WriteLine("ORACLE " + JsonSerializer.Serialize(compact));
            return 0;
        }

        var runResults = new List<object>();
        string sha = "";
        long jsonBytes = 0;
        for (int run = 0; run < runs; run++)
        {
            var allocBefore = GC.GetTotalAllocatedBytes(true);
            sw.Restart();
            var parser = new EvtcParser(settings, api);
            var log = parser.ParseLog(new Controller(), new FileInfo(logPath), out var failure, threads);
            var parseMs = sw.Elapsed.TotalMilliseconds;
            if (log == null) { Console.Error.WriteLine($"Parse fehlgeschlagen: {failure?.Reason}"); return 1; }

            double buildMs = 0, jsonMs = 0;
            if (!noJson)
            {
                sw.Restart();
                var builder = new RawFormatBuilder(log, new RawFormatSettings(), new Version(1, 0), new UploadResults(""));
                buildMs = sw.Elapsed.TotalMilliseconds;
                sw.Restart();
                using var ms = new MemoryStream();
                builder.CreateJSON(ms, false);
                jsonMs = sw.Elapsed.TotalMilliseconds;
                jsonBytes = ms.Length;
                sha = Convert.ToHexString(SHA256.HashData(ms.GetBuffer().AsSpan(0, (int)ms.Length)));
                if (outPath != null && run == runs - 1)
                {
                    File.WriteAllBytes(outPath, ms.ToArray());
                }
            }
            runResults.Add(new
            {
                run = run + 1,
                parse_ms = Math.Round(parseMs, 1),
                build_ms = Math.Round(buildMs, 1),
                json_ms = Math.Round(jsonMs, 1),
                total_ms = Math.Round(parseMs + buildMs + jsonMs, 1),
                allocated_mb = Math.Round((GC.GetTotalAllocatedBytes(true) - allocBefore) / 1048576.0, 1),
            });
        }

        var proc = Process.GetCurrentProcess();
        var result = new
        {
            log = Path.GetFileName(logPath),
            threads,
            runtime_start_ms = Math.Round(sinceStartMs, 1),
            api_load_ms = Math.Round(apiMs, 1),
            runs = runResults,
            peak_working_set_mb = Math.Round(proc.PeakWorkingSet64 / 1048576.0, 1),
            gc_gen0 = GC.CollectionCount(0),
            gc_gen1 = GC.CollectionCount(1),
            gc_gen2 = GC.CollectionCount(2),
            json_bytes = jsonBytes,
            json_sha256 = sha,
            wall_since_process_start_ms = Math.Round((DateTime.Now - processStart).TotalMilliseconds, 1),
        };
        Console.WriteLine("RESULT " + JsonSerializer.Serialize(result));
        return 0;
    }
}
