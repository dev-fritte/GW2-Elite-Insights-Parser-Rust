using System.Globalization;
using System.Reflection;
using System.Text;
using GW2EIEvtcParser;
using GW2EIEvtcParser.ParsedData;
using GW2EIGW2API;

namespace GW2EIParserReference;

/// <summary>
/// Phase 7 extractor: for every trigger id the logic that C# detects and its target, trash and friendly
/// NPC lists, as a tab separated file (by reflection, so the lists are the ones the parser really uses).
/// Aufruf: GW2EIParserReference --dump-targets &lt;datei&gt;
/// </summary>
internal static class TargetDump
{
    private const BindingFlags Any = BindingFlags.Instance | BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic;

    public static void Run(string file)
    {
        var asm = typeof(EvtcParser).Assembly;
        Type T(string name) => asm.GetTypes().First(t => t.Name == name);
        var targetIdType = T("TargetID");
        var agentDataType = T("AgentData");
        var holderType = T("UniqueAgentIDHolder");
        var logDataType = T("LogData");
        var logicType = T("LogLogic");
        var detect = logDataType.GetMethod("DetectLogic", Any)!;
        var settings = new EvtcParserSettings(2200, 150);
        var versionType = T("EvtcVersionEvent");
        var version = Activator.CreateInstance(versionType, Any, null, new object[] { 20260501 }, null)!;
        var content = Path.Combine(AppContext.BaseDirectory, "Content");
        var api = new GW2APIController(
            Path.Combine(content, "SkillList.json"), Path.Combine(content, "SpecList.json"),
            Path.Combine(content, "TraitList.json"), Path.Combine(content, "MapList.json"));
        var add = agentDataType.GetMethods(Any).First(m => m.Name == "AddCustomNPCAgent" && m.GetParameters()[4].ParameterType == typeof(int));
        var sb = new StringBuilder("Trigger\tLogic\tKind\tIndex\tId\n");
        var seen = new HashSet<int>();
        foreach (var raw in Enum.GetValues(targetIdType))
        {
            int id = Convert.ToInt32(raw, CultureInfo.InvariantCulture);
            if (id <= 0 || !seen.Add(id)) { continue; }
            try
            {
                var holder = Activator.CreateInstance(holderType, Any, null, null, null)!;
                var ad = Activator.CreateInstance(agentDataType, Any, null, new object[] { api, new List<AgentItem>(), holder }, null)!;
                add.Invoke(ad, new object?[] { 0L, 100L, "x", ParserHelper.Spec.NPC, id, false, (ushort)0, (ushort)0, (ushort)0, (ushort)0, 0u, 0u });
                object logic;
                if (id is 43488 or 43974 or 47188)
                {
                    // gadget logics: their trigger species is a volatile agent in real logs
                    var name = id == 43488 ? "HarvestTemple" : id == 43974 ? "ConjuredAmalgamate" : "SpiritRace";
                    logic = Activator.CreateInstance(T(name), Any, null, new object[] { id }, null)!;
                }
                else
                {
                    logic = detect.Invoke(null, new object[] { id, ad, settings, version })!;
                }
                foreach (var (kind, method) in new[] { ("target", "GetTargetsIDs"), ("trash", "GetTrashMobsIDs"), ("friendly", "GetFriendlyNPCIDs") })
                {
                    var list = (System.Collections.IEnumerable)logicType.GetMethod(method, Any)!.Invoke(logic, null)!;
                    int i = 0;
                    foreach (var e in list)
                    {
                        sb.Append(id).Append('\t').Append(logic.GetType().Name).Append('\t').Append(kind).Append('\t').Append(i++).Append('\t').Append(Convert.ToInt32(e, CultureInfo.InvariantCulture)).Append('\n');
                    }
                }
                var sortIds = (System.Collections.IDictionary)logicType.GetMethod("GetTargetsSortIDs", Any)!.Invoke(logic, null)!;
                foreach (System.Collections.DictionaryEntry e in sortIds)
                {
                    sb.Append(id).Append('\t').Append(logic.GetType().Name).Append('\t').Append("sort").Append('\t').Append(Convert.ToInt32(e.Value, CultureInfo.InvariantCulture)).Append('\t').Append(Convert.ToInt32(e.Key, CultureInfo.InvariantCulture)).Append('\n');
                }            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"{id}: {(ex.InnerException ?? ex).Message}");
            }
        }
        File.WriteAllText(file, sb.ToString());
    }
}
