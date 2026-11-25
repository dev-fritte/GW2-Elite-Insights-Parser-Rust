using System.Globalization;
using System.Reflection;
using System.Text;
using GW2EIEvtcParser;
using GW2EIEvtcParser.EIData;

namespace GW2EIParserReference;

/// <summary>
/// Phase 4 extractor: writes the data tables of the parser as tab separated files by reflection,
/// so the values are the ones the C# parser really uses (constants resolved, no source parsing).
/// Aufruf: GW2EIParserReference --dump-data &lt;verzeichnis&gt;
/// </summary>
internal static class DataDump
{
    private const BindingFlags All = BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly;

    private static T Field<T>(object o, string name)
    {
        var f = o.GetType().GetField(name, BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic)
            ?? throw new InvalidOperationException($"{o.GetType().Name}.{name} fehlt");
        return (T)f.GetValue(o)!;
    }

    private static string Clean(string s) => s.Replace('\t', ' ').Replace('\r', ' ').Replace('\n', ' ');

    /// <summary>All static members of the parser assembly that hold a collection of <see cref="Buff"/>.</summary>
    private static IEnumerable<(string Owner, IEnumerable<Buff> Buffs)> BuffLists()
    {
        foreach (var type in typeof(EvtcParser).Assembly.GetTypes().OrderBy(t => t.FullName, StringComparer.Ordinal))
        {
            foreach (var member in type.GetMembers(All).OrderBy(m => m.Name, StringComparer.Ordinal))
            {
                Type? memberType = member switch { FieldInfo f => f.FieldType, PropertyInfo p when p.GetIndexParameters().Length == 0 => p.PropertyType, _ => null };
                if (memberType == null || !typeof(IEnumerable<Buff>).IsAssignableFrom(memberType) || memberType == typeof(string)) { continue; }
                object? value = member switch { FieldInfo f => f.GetValue(null), PropertyInfo p => p.GetValue(null), _ => null };
                if (value is IEnumerable<Buff> buffs) { yield return ($"{type.Name}.{member.Name}", buffs); }
            }
        }
    }

    public static void Run(string dir)
    {
        Directory.CreateDirectory(dir);
        var inv = CultureInfo.InvariantCulture;
        var sb = new StringBuilder();
        sb.Append("List\tName\tId\tClassification\tSources\tStackType\tCapacity\tMinBuild\tMaxBuild\tMinEvtcBuild\tMaxEvtcBuild\tLink\n");
        int rows = 0;
        foreach (var (owner, buffs) in BuffLists())
        {
            foreach (var b in buffs)
            {
                var sources = b.Sources.Select(s => s.ToString()).OrderBy(s => s, StringComparer.Ordinal);
                sb.Append(owner).Append('\t')
                  .Append(Clean(b.Name)).Append('\t')
                  .Append(b.ID.ToString(inv)).Append('\t')
                  .Append(b.Classification).Append('\t')
                  .Append(string.Join('|', sources)).Append('\t')
                  .Append(b.StackType).Append('\t')
                  .Append(b.Capacity.ToString(inv)).Append('\t')
                  .Append(Field<ulong>(b, "_minBuild").ToString(inv)).Append('\t')
                  .Append(Field<ulong>(b, "_maxBuild").ToString(inv)).Append('\t')
                  .Append(Field<int>(b, "_minEvtcBuild").ToString(inv)).Append('\t')
                  .Append(Field<int>(b, "_maxEvtcBuild").ToString(inv)).Append('\t')
                  .Append(Clean(b.Link ?? "")).Append('\n');
                rows++;
            }
        }
        File.WriteAllText(Path.Combine(dir, "buffs.tsv"), sb.ToString(), new UTF8Encoding(false));
        Console.WriteLine($"buffs.tsv {rows} rows");
    }
}
