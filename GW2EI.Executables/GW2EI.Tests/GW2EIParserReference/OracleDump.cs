using System.IO.Compression;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using GW2EIEvtcParser;
using GW2EIEvtcParser.ParsedData;
using GW2EIEvtcParser.ParserHelpers;

namespace GW2EIParserReference;

/// <summary>
/// Phase 1.0: oracle for the Rust ingest crate.
/// Reads a log with the original C# readers of <see cref="EvtcParser"/> (private methods, called by reflection so the library stays untouched)
/// and reduces it to counts, scalars and SHA256 hashes over a canonical byte layout. See docs/phase1/ORACLE.md.
///
/// Layer A: what is in the file (header, agent table, skill table, combat records), before any filtering.
/// Layer B: what ParseCombatList keeps (validity filter, time offset), as scalars plus a hash.
/// </summary>
internal static class OracleDump
{
    private sealed class Controller : ParserController
    {
        public override void UpdateProgressWithCancellationCheck(string status) { }
    }

    private const BindingFlags Static = BindingFlags.NonPublic | BindingFlags.Static;
    private const BindingFlags Inst = BindingFlags.NonPublic | BindingFlags.Instance;

    private delegate CombatItem ReadItem(BinaryReader reader, EvtcVersionEvent version);

    public static byte[] LoadEvtc(string path)
    {
        if (SupportedFileFormats.IsCompressedFormat(path))
        {
            using var fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read);
            using var arch = new ZipArchive(fs, ZipArchiveMode.Read);
            if (arch.Entries.Count != 1) { throw new InvalidDataException("Invalid Archive"); }
            using var data = arch.Entries[0].Open();
            using var ms = new MemoryStream();
            data.CopyTo(ms);
            return ms.ToArray();
        }
        return File.ReadAllBytes(path);
    }

    /// <summary>Writes the 64 byte canonical record of an event: all decoded fields, little endian, in file order.</summary>
    private static void WriteRecord(Span<byte> b, CombatItem c)
    {
        int o = 0;
        BitConverter.TryWriteBytes(b.Slice(o, 8), c.Time); o += 8;
        BitConverter.TryWriteBytes(b.Slice(o, 8), c.SrcAgent); o += 8;
        BitConverter.TryWriteBytes(b.Slice(o, 8), c.DstAgent); o += 8;
        BitConverter.TryWriteBytes(b.Slice(o, 4), c.Value); o += 4;
        BitConverter.TryWriteBytes(b.Slice(o, 4), c.BuffDmg); o += 4;
        BitConverter.TryWriteBytes(b.Slice(o, 4), c.OverstackValue); o += 4;
        BitConverter.TryWriteBytes(b.Slice(o, 4), c.SkillID); o += 4;
        BitConverter.TryWriteBytes(b.Slice(o, 2), c.SrcInstid); o += 2;
        BitConverter.TryWriteBytes(b.Slice(o, 2), c.DstInstid); o += 2;
        BitConverter.TryWriteBytes(b.Slice(o, 2), c.SrcMasterInstid); o += 2;
        BitConverter.TryWriteBytes(b.Slice(o, 2), c.DstMasterInstid); o += 2;
        b[o++] = c.IFFByte;
        b[o++] = c.IsBuff;
        b[o++] = c.Result;
        b[o++] = c.IsActivationByte;
        b[o++] = c.IsBuffRemoveByte;
        b[o++] = c.IsNinety;
        b[o++] = c.IsFifty;
        b[o++] = c.IsMoving;
        b[o++] = (byte)c.IsStateChange;
        b[o++] = c.IsFlanking;
        b[o++] = c.IsShields;
        b[o++] = c.IsOffcycle;
        BitConverter.TryWriteBytes(b.Slice(o, 4), c.Pad); o += 4;
        if (o != 64) { throw new InvalidOperationException("record size"); }
    }

    private static string Hex(byte[] h) => Convert.ToHexString(h);

    public static (object Compact, string Full) Run(string path, EvtcParser parser)
    {
        var bytes = LoadEvtc(path);
        var controller = new Controller();
        using var ms = new MemoryStream(bytes, writable: false);
        using var reader = new BinaryReader(ms, new UTF8Encoding(), leaveOpen: true);
        var t = typeof(EvtcParser);

        // ---- header
        var header = (ITuple)t.GetMethod("ParseLogData", Static)!.Invoke(null, [reader, controller])!;
        byte revision = (byte)header[0]!;
        ushort triggerId = (ushort)header[1]!;
        var version = (EvtcVersionEvent)header[2]!;
        int headerBuild = version.Build;
        string buildString = Encoding.ASCII.GetString(bytes, 0, 12);

        // ---- agents (C# reader), plus raw prof / is_elite read straight from the record
        long agentStart = ms.Position;
        uint agentCount = BitConverter.ToUInt32(bytes, (int)agentStart);
        var agents = (System.Collections.IList)t.GetMethod("ParseAgentData", Inst)!.Invoke(parser, [reader, controller])!;
        using var agentHash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        var agentRows = new List<object>();
        for (int i = 0; i < agents.Count; i++)
        {
            var a = (AgentItem)agents[i]!;
            int rec = (int)agentStart + 4 + 96 * i;
            uint rawProf = BitConverter.ToUInt32(bytes, rec + 8);
            uint rawElite = BitConverter.ToUInt32(bytes, rec + 12);
            // The AgentItem constructor overrides the hitbox of players (48 x 240), so read the raw values.
            uint rawHitboxW = 2u * BitConverter.ToUInt16(bytes, rec + 22);
            uint rawHitboxH = 2u * BitConverter.ToUInt16(bytes, rec + 26);
            var name = Encoding.UTF8.GetBytes(a.Name);
            using var buf = new MemoryStream();
            using (var w = new BinaryWriter(buf, Encoding.UTF8, leaveOpen: true))
            {
                w.Write(a.Agent); w.Write(rawProf); w.Write(rawElite);
                w.Write(a.Toughness); w.Write(a.Concentration); w.Write(a.Healing); w.Write(a.Condition);
                w.Write(rawHitboxW); w.Write(rawHitboxH);
                w.Write((uint)name.Length); w.Write(name);
            }
            agentHash.AppendData(buf.ToArray());
            agentRows.Add(new { agent = a.Agent, prof = rawProf, elite = rawElite, toughness = a.Toughness, concentration = a.Concentration, healing = a.Healing, condition = a.Condition, hitbox_w = rawHitboxW, hitbox_h = rawHitboxH, name = a.Name });
        }

        // ---- skills (C# reader: first occurrence wins, name cut at first NUL)
        long skillStart = ms.Position;
        uint skillCountRaw = BitConverter.ToUInt32(bytes, (int)skillStart);
        var skills = (Dictionary<long, string>)t.GetMethod("ParseSkillData", Static)!.Invoke(null, [reader, controller, version])!;
        using var skillHash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        var skillRows = new List<object>();
        foreach (var (id, name) in skills)
        {
            var nb = Encoding.UTF8.GetBytes(name);
            skillHash.AppendData(BitConverter.GetBytes((int)id));
            skillHash.AppendData(BitConverter.GetBytes((uint)nb.Length));
            skillHash.AppendData(nb);
            skillRows.Add(new { id, name });
        }

        // ---- combat records (layer A): every 64 byte record, no filtering, times untouched
        long eventStart = ms.Position;
        long eventBytes = bytes.Length - eventStart;
        long eventCount = eventBytes / 64;
        long trailing = eventBytes % 64;
        var readName = revision > 0 ? "ReadCombatItemRev1" : "ReadCombatItem";
        var read = t.GetMethod(readName, Static)!.CreateDelegate<ReadItem>();
        using var evHash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        var hist = new long[256];
        var rec64 = new byte[64];
        long firstTime = long.MaxValue, lastTime = long.MinValue;
        var head = new List<object>();
        var tail = new Queue<object>();
        for (long i = 0; i < eventCount; i++)
        {
            var c = read(reader, version);
            WriteRecord(rec64, c);
            evHash.AppendData(rec64);
            hist[(byte)c.IsStateChange]++;
            if (c.Time < firstTime) { firstTime = c.Time; }
            if (c.Time > lastTime) { lastTime = c.Time; }
            if (i < 3 || i >= eventCount - 3)
            {
                var row = new { i, time = c.Time, src = c.SrcAgent, dst = c.DstAgent, value = c.Value, skill = c.SkillID, statechange = (byte)c.IsStateChange, hex = Convert.ToHexString(rec64) };
                if (i < 3) { head.Add(row); } else { tail.Enqueue(row); }
            }
        }

        // ---- layer B: what ParseCombatList keeps
        ms.Position = 0;
        var version2 = (EvtcVersionEvent)((ITuple)t.GetMethod("ParseLogData", Static)!.Invoke(null, [reader, controller])!)[2]!;
        ms.Position = eventStart;
        object? kept = null;
        string? keptError = null;
        try
        {
            var res = (ITuple)t.GetMethod("ParseCombatList", Inst)!.Invoke(parser, [reader, controller, revision, triggerId, skills, version2])!;
            var items = (IReadOnlyList<CombatItem>)res[0]!;
            using var keptHash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
            foreach (var c in items) { WriteRecord(rec64, c); keptHash.AppendData(rec64); }
            kept = new
            {
                count = items.Count,
                hash = Hex(keptHash.GetHashAndReset()),
                trigger_id = (ushort)res[4]!,
                map_id = (int)res[5]!,
                log_start_offset = (long)res[6]!,
                log_end_time = (long)res[7]!,
                gw2_build = (ulong)res[8]!,
            };
        }
        catch (TargetInvocationException ex)
        {
            keptError = ex.InnerException?.GetType().Name + ": " + ex.InnerException?.Message;
        }

        var histObj = new SortedDictionary<int, long>();
        for (int i = 0; i < 256; i++) { if (hist[i] != 0) { histObj[i] = hist[i]; } }

        var compact = new
        {
            file = Path.GetFileName(path),
            bytes = bytes.Length,
            header = new { build_string = buildString, evtc_version = headerBuild, revision, trigger_id = triggerId },
            agents = new { count = agentCount, parsed = agents.Count, hash = Hex(agentHash.GetHashAndReset()) },
            skills = new { count_raw = skillCountRaw, unique = skills.Count, hash = Hex(skillHash.GetHashAndReset()) },
            events = new { count = eventCount, trailing_bytes = trailing, hash = Hex(evHash.GetHashAndReset()), first_time = firstTime, last_time = lastTime },
            kept,
            kept_error = keptError,
        };
        var full = new
        {
            compact,
            statechange_histogram = histObj,
            events_head = head,
            events_tail = tail,
            agents = agentRows,
            skills = skillRows,
        };
        return (compact, JsonSerializer.Serialize(full, new JsonSerializerOptions { WriteIndented = true }));
    }
}
