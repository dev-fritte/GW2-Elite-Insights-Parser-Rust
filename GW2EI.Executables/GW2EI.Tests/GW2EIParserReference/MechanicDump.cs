using System.Collections;
using System.Globalization;
using System.Reflection;
using System.Text;
using GW2EIEvtcParser;
using GW2EIEvtcParser.EIData;
using GW2EIEvtcParser.ParsedData;
using GW2EIGW2API;

namespace GW2EIParserReference;

/// <summary>
/// Phase 7 extractor: the encounter mechanics of every logic (type, skill ids, names and the flags that
/// say whether a lambda or other extra is attached), as a tab separated file.
/// Aufruf: GW2EIParserReference --dump-mechanics &lt;datei&gt;
/// </summary>
internal static class MechanicDump
{
    private const BindingFlags Any = BindingFlags.Instance | BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic;

    private static object? Field(object o, string name)
    {
        for (var t = o.GetType(); t != null; t = t.BaseType)
        {
            var f = t.GetField(name, Any | BindingFlags.DeclaredOnly);
            if (f != null) { return f.GetValue(o); }
            var p = t.GetProperty(name, Any | BindingFlags.DeclaredOnly);
            if (p != null) { return p.GetValue(o); }
        }
        return null;
    }

    private static int Count(object? list) => list is ICollection c ? c.Count : 0;

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
        var listField = logicType.GetField("MechanicList", Any)!;
        var basic = logicType.GetField("_basicMechanicsCount", Any)!;
        var sb = new StringBuilder("Trigger\tLogic\tType\tIds\tShort\tIcd\tSeverity\tEnemy\tIgnored\tCheckers\tSubs\tClamper\tWeighted\tMinions\tEnables\tMinBuild\tMaxBuild\tMinEvtc\tMaxEvtc\tFull\n");
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
                    var name = id == 43488 ? "HarvestTemple" : id == 43974 ? "ConjuredAmalgamate" : "SpiritRace";
                    logic = Activator.CreateInstance(T(name), Any, null, new object[] { id }, null)!;
                }
                else
                {
                    logic = detect.Invoke(null, new object[] { id, ad, settings, version })!;
                }
                var containers = (IList)listField.GetValue(logic)!;
                int skip = (int)basic.GetValue(logic)!;
                for (int i = skip; i < containers.Count; i++)
                {
                    foreach (var m in ((MechanicContainer)containers[i]!).GetMechanics())
                    {
                        var ids = Field(m, "MechanicIDs") as IEnumerable;
                        var idText = ids == null ? "" : string.Join(",", ids.Cast<object>().Select(x => x is Guid g ? g.ToString() : Convert.ToInt64(x, CultureInfo.InvariantCulture).ToString(CultureInfo.InvariantCulture)));
                        var enables = Field(m, "_enableConditions");
                        sb.Append(id).Append('\t').Append(logic.GetType().Name).Append('\t').Append(m.GetType().Name).Append('\t').Append(idText).Append('\t')
                          .Append(m.ShortName.Replace('\t', ' ')).Append('\t').Append(m.InternalCooldown).Append('\t').Append(m.Severity).Append('\t')
                          .Append(m.IsEnemyMechanic ? 1 : 0).Append('\t').Append(m.Ignored ? 1 : 0).Append('\t')
                          .Append(Count(Field(m, "Checkers"))).Append('\t').Append(Count(Field(m, "_subMechanics"))).Append('\t')
                          .Append(Field(m, "_timeClamper") != null ? 1 : 0).Append('\t').Append(Field(m, "_weighted") is true ? 1 : 0).Append('\t')
                          .Append(Field(m, "Minions") is true ? 1 : 0).Append('\t').Append(Count(enables)).Append('\t')
                          .Append(Field(m, "_minBuild")).Append('\t').Append(Field(m, "_maxBuild")).Append('\t')
                          .Append(Field(m, "_minEvtcBuild")).Append('\t').Append(Field(m, "_maxEvtcBuild")).Append('\t')
                          .Append(m.FullName.Replace('\t', ' ')).Append('\n');
                    }
                }
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine($"{id}: {(ex.InnerException ?? ex).Message}");
            }
        }
        File.WriteAllText(file, sb.ToString());
    }
}
