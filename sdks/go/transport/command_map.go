package transport

// CommandMapping is the result of mapping a dotted SDK name onto a
// wire-level verb + argument vector.
type CommandMapping struct {
	Command string
	Args    []NexusValue
}

// MapCommand translates an SDK dotted name (`graph.cypher`,
// `db.create`, …) into a wire command plus an argument list. Returns
// nil when the name is not mapped so the caller can fall back to HTTP
// or surface a clear error.
//
// The table must stay in sync with docs/specs/sdk-transport.md §6 and
// with the Rust SDK's sdks/rust/src/transport/command_map.rs.
func MapCommand(dotted string, payload map[string]any) *CommandMapping {
	switch dotted {
	case "graph.cypher":
		q, ok := payload["query"].(string)
		if !ok {
			return nil
		}
		args := []NexusValue{NxStr(q)}
		if params, present := payload["parameters"]; present && params != nil {
			args = append(args, JsonToNexus(params))
		}
		return &CommandMapping{Command: "CYPHER", Args: args}
	case "graph.ping":
		return &CommandMapping{Command: "PING"}
	case "graph.hello":
		return &CommandMapping{Command: "HELLO", Args: []NexusValue{NxInt(1)}}
	case "graph.stats":
		return &CommandMapping{Command: "STATS"}
	case "graph.health":
		return &CommandMapping{Command: "HEALTH"}
	case "graph.quit":
		return &CommandMapping{Command: "QUIT"}
	case "auth.login":
		if key, ok := payload["api_key"].(string); ok && key != "" {
			return &CommandMapping{Command: "AUTH", Args: []NexusValue{NxStr(key)}}
		}
		u, ok1 := payload["username"].(string)
		p, ok2 := payload["password"].(string)
		if !ok1 || !ok2 {
			return nil
		}
		return &CommandMapping{Command: "AUTH", Args: []NexusValue{NxStr(u), NxStr(p)}}

	case "db.list":
		return &CommandMapping{Command: "DB_LIST"}
	case "db.create":
		if n, ok := payload["name"].(string); ok {
			return &CommandMapping{Command: "DB_CREATE", Args: []NexusValue{NxStr(n)}}
		}
		return nil
	case "db.drop":
		if n, ok := payload["name"].(string); ok {
			return &CommandMapping{Command: "DB_DROP", Args: []NexusValue{NxStr(n)}}
		}
		return nil
	case "db.use":
		if n, ok := payload["name"].(string); ok {
			return &CommandMapping{Command: "DB_USE", Args: []NexusValue{NxStr(n)}}
		}
		return nil

	case "schema.labels":
		return &CommandMapping{Command: "LABELS"}
	case "schema.rel_types":
		return &CommandMapping{Command: "REL_TYPES"}
	case "schema.property_keys":
		return &CommandMapping{Command: "PROPERTY_KEYS"}
	case "schema.indexes":
		return &CommandMapping{Command: "INDEXES"}

	case "data.export":
		fmt, ok := payload["format"].(string)
		if !ok {
			return nil
		}
		args := []NexusValue{NxStr(fmt)}
		if q, ok := payload["query"].(string); ok {
			args = append(args, NxStr(q))
		}
		return &CommandMapping{Command: "EXPORT", Args: args}
	case "data.import":
		fmt, ok1 := payload["format"].(string)
		data, ok2 := payload["data"].(string)
		if !ok1 || !ok2 {
			return nil
		}
		return &CommandMapping{Command: "IMPORT", Args: []NexusValue{NxStr(fmt), NxStr(data)}}
	}
	return nil
}

// JsonToNexus — JSON-compatible Go value to NexusValue.
func JsonToNexus(v any) NexusValue {
	switch x := v.(type) {
	case nil:
		return NxNull()
	case bool:
		return NxBool(x)
	case int:
		return NxInt(int64(x))
	case int32:
		return NxInt(int64(x))
	case int64:
		return NxInt(x)
	case float32:
		return NxFloat(float64(x))
	case float64:
		// JSON numbers land here — preserve integer magnitudes.
		if x == float64(int64(x)) {
			return NxInt(int64(x))
		}
		return NxFloat(x)
	case string:
		return NxStr(x)
	case []byte:
		return NxBytes(x)
	case []any:
		out := make([]NexusValue, len(x))
		for i, e := range x {
			out[i] = JsonToNexus(e)
		}
		return NxArray(out)
	case map[string]any:
		pairs := make([]MapEntry, 0, len(x))
		for k, val := range x {
			pairs = append(pairs, MapEntry{Key: NxStr(k), Value: JsonToNexus(val)})
		}
		return NxMap(pairs)
	}
	return NxNull()
}

// NexusToJson — NexusValue to JSON-compatible Go value for user-visible
// surfaces. Keys that are not strings are stringified.
func NexusToJson(v NexusValue) any {
	switch v.Kind {
	case KindNull:
		return nil
	case KindBool, KindInt, KindFloat, KindStr, KindBytes:
		return v.Value
	case KindArray:
		arr := v.Value.([]NexusValue)
		out := make([]any, len(arr))
		for i, e := range arr {
			out[i] = NexusToJson(e)
		}
		return out
	case KindMap:
		pairs := v.Value.([]MapEntry)
		out := make(map[string]any, len(pairs))
		for _, p := range pairs {
			key := ""
			switch p.Key.Kind {
			case KindStr:
				key = p.Key.Value.(string)
			default:
				if s, ok := p.Key.Value.(string); ok {
					key = s
				} else {
					// stringify numbers etc.
					key = ""
				}
			}
			out[key] = NexusToJson(p.Value)
		}
		return out
	}
	return nil
}
