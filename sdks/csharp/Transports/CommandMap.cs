namespace Nexus.SDK.Transports;

/// <summary>
/// SDK dotted-name → wire-command mapping.
///
/// Every method <c>NexusClient</c> exposes funnels through
/// <see cref="Map"/>. The table must stay in sync with
/// <c>docs/specs/sdk-transport.md §6</c> and with the Rust SDK's
/// <c>sdks/rust/src/transport/command_map.rs</c>.
/// </summary>
public static class CommandMap
{
    public class Mapping
    {
        public string Command { get; }
        public List<NexusValue> Args { get; }

        public Mapping(string command, List<NexusValue> args)
        {
            Command = command;
            Args = args;
        }
    }

    public static Mapping? Map(string dotted, IReadOnlyDictionary<string, object?> payload)
    {
        switch (dotted)
        {
            case "graph.cypher":
                if (!payload.TryGetValue("query", out var q) || q is not string qs) return null;
                var args = new List<NexusValue> { NexusValue.Str(qs) };
                if (payload.TryGetValue("parameters", out var p) && p != null)
                    args.Add(JsonToNexus(p));
                return new Mapping("CYPHER", args);
            case "graph.ping":
                return new Mapping("PING", new List<NexusValue>());
            case "graph.hello":
                return new Mapping("HELLO", new List<NexusValue> { NexusValue.Int(1) });
            case "graph.stats":
                return new Mapping("STATS", new List<NexusValue>());
            case "graph.health":
                return new Mapping("HEALTH", new List<NexusValue>());
            case "graph.quit":
                return new Mapping("QUIT", new List<NexusValue>());
            case "auth.login":
                {
                    if (payload.TryGetValue("api_key", out var k) && k is string ks && ks.Length > 0)
                        return new Mapping("AUTH", new List<NexusValue> { NexusValue.Str(ks) });
                    if (!payload.TryGetValue("username", out var u) || u is not string us) return null;
                    if (!payload.TryGetValue("password", out var pw) || pw is not string ps) return null;
                    return new Mapping("AUTH", new List<NexusValue>
                    {
                        NexusValue.Str(us), NexusValue.Str(ps),
                    });
                }
            case "db.list":
                return new Mapping("DB_LIST", new List<NexusValue>());
            case "db.create":
            case "db.drop":
            case "db.use":
                if (!payload.TryGetValue("name", out var n) || n is not string ns) return null;
                var cmd = dotted switch
                {
                    "db.create" => "DB_CREATE",
                    "db.drop" => "DB_DROP",
                    _ => "DB_USE",
                };
                return new Mapping(cmd, new List<NexusValue> { NexusValue.Str(ns) });
            case "schema.labels":
                return new Mapping("LABELS", new List<NexusValue>());
            case "schema.rel_types":
                return new Mapping("REL_TYPES", new List<NexusValue>());
            case "schema.property_keys":
                return new Mapping("PROPERTY_KEYS", new List<NexusValue>());
            case "schema.indexes":
                return new Mapping("INDEXES", new List<NexusValue>());
            case "data.export":
                {
                    if (!payload.TryGetValue("format", out var f) || f is not string fs) return null;
                    var a = new List<NexusValue> { NexusValue.Str(fs) };
                    if (payload.TryGetValue("query", out var eq) && eq is string eqs)
                        a.Add(NexusValue.Str(eqs));
                    return new Mapping("EXPORT", a);
                }
            case "data.import":
                {
                    if (!payload.TryGetValue("format", out var f) || f is not string fs) return null;
                    if (!payload.TryGetValue("data", out var d) || d is not string ds) return null;
                    return new Mapping("IMPORT", new List<NexusValue>
                    {
                        NexusValue.Str(fs), NexusValue.Str(ds),
                    });
                }
        }
        return null;
    }

    /// <summary>Convert a JSON-compatible value into a <see cref="NexusValue"/>.</summary>
    public static NexusValue JsonToNexus(object? v)
    {
        switch (v)
        {
            case null: return NexusValue.Null();
            case bool b: return NexusValue.Bool(b);
            case byte ub: return NexusValue.Int(ub);
            case sbyte sb: return NexusValue.Int(sb);
            case short s: return NexusValue.Int(s);
            case ushort us: return NexusValue.Int(us);
            case int i: return NexusValue.Int(i);
            case uint ui: return NexusValue.Int(ui);
            case long l: return NexusValue.Int(l);
            case ulong ul: return NexusValue.Int((long)ul);
            case float f: return NexusValue.Float(f);
            case double d: return NexusValue.Float(d);
            case string str: return NexusValue.Str(str);
            case byte[] bytes: return NexusValue.Bytes(bytes);
        }
        if (v is IEnumerable<KeyValuePair<string, object?>> keyValueEnumerable)
        {
            var pairs = new List<(NexusValue, NexusValue)>();
            foreach (var kv in keyValueEnumerable)
                pairs.Add((NexusValue.Str(kv.Key), JsonToNexus(kv.Value)));
            return NexusValue.Map(pairs);
        }
        if (v is System.Collections.IEnumerable enumerable && v is not string)
        {
            var arr = new List<NexusValue>();
            foreach (var e in enumerable) arr.Add(JsonToNexus(e));
            return NexusValue.Array(arr);
        }
        return NexusValue.Null();
    }

    /// <summary>Convert a <see cref="NexusValue"/> into a JSON-compatible value.</summary>
    public static object? NexusToJson(NexusValue v)
    {
        switch (v.Kind)
        {
            case NexusValueKind.Null: return null;
            case NexusValueKind.Bool:
            case NexusValueKind.Int:
            case NexusValueKind.Float:
            case NexusValueKind.Str:
            case NexusValueKind.Bytes:
                return v.Value;
            case NexusValueKind.Array:
                var arr = (List<NexusValue>)v.Value!;
                var outArr = new List<object?>(arr.Count);
                foreach (var e in arr) outArr.Add(NexusToJson(e));
                return outArr;
            case NexusValueKind.Map:
                var pairs = (List<(NexusValue Key, NexusValue Value)>)v.Value!;
                var outObj = new Dictionary<string, object?>(pairs.Count);
                foreach (var (k, val) in pairs)
                {
                    var key = k.Kind == NexusValueKind.Str
                        ? (string)k.Value!
                        : k.Value?.ToString() ?? "";
                    outObj[key] = NexusToJson(val);
                }
                return outObj;
        }
        return null;
    }
}
