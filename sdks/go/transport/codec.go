package transport

import (
	"bytes"
	"encoding/binary"
	"fmt"

	"github.com/vmihailenco/msgpack/v5"
)

// Wire codec for the Nexus RPC protocol.
//
// The server-side types live in `nexus_protocol::rpc::types`.
// rmp-serde encodes Rust enums using the externally-tagged
// representation by default, which means:
//
//   - Unit variants (`NexusValue::Null`) → the string "Null".
//   - Data-bearing variants (`NexusValue::Str("hi")`) → a single-key
//     map {"Str": "hi"}.
//   - `Result<T, E>` → {"Ok": v} / {"Err": s}.
//
// ToWire / FromWire translate between the tagged Go type and the
// on-wire shape. EncodeRequestFrame / DecodeResponseBody handle the
// `u32 LE length prefix + MessagePack` frame format.

// ToWire encodes a [NexusValue] into its on-wire (pre-MessagePack)
// Go shape — either the literal string "Null" or a single-key map.
func ToWire(v NexusValue) any {
	switch v.Kind {
	case KindNull:
		return "Null"
	case KindBool:
		return map[string]any{"Bool": v.Value}
	case KindInt:
		return map[string]any{"Int": v.Value}
	case KindFloat:
		return map[string]any{"Float": v.Value}
	case KindBytes:
		return map[string]any{"Bytes": v.Value}
	case KindStr:
		return map[string]any{"Str": v.Value}
	case KindArray:
		arr := v.Value.([]NexusValue)
		out := make([]any, len(arr))
		for i, e := range arr {
			out[i] = ToWire(e)
		}
		return map[string]any{"Array": out}
	case KindMap:
		pairs := v.Value.([]MapEntry)
		out := make([]any, len(pairs))
		for i, p := range pairs {
			out[i] = []any{ToWire(p.Key), ToWire(p.Value)}
		}
		return map[string]any{"Map": out}
	}
	panic(fmt.Sprintf("unknown NexusValue kind '%s'", v.Kind))
}

// FromWire decodes the wire-level Go shape back into a tagged [NexusValue].
//
// Accepts both the tagged shape ({"Str": "hi"}) and un-tagged primitives
// (bare bool / int / string / []byte / []any) so the codec tolerates
// servers (or tests) that emit the raw msgpack form directly.
func FromWire(raw any) (NexusValue, error) {
	switch x := raw.(type) {
	case nil:
		return NxNull(), nil
	case string:
		if x == "Null" {
			return NxNull(), nil
		}
		return NxStr(x), nil
	case bool:
		return NxBool(x), nil
	case int:
		return NxInt(int64(x)), nil
	case int64:
		return NxInt(x), nil
	case int32:
		return NxInt(int64(x)), nil
	case int16:
		return NxInt(int64(x)), nil
	case int8:
		return NxInt(int64(x)), nil
	case uint:
		return NxInt(int64(x)), nil
	case uint64:
		return NxInt(int64(x)), nil
	case uint32:
		return NxInt(int64(x)), nil
	case uint16:
		return NxInt(int64(x)), nil
	case uint8:
		return NxInt(int64(x)), nil
	case float32:
		return NxFloat(float64(x)), nil
	case float64:
		return NxFloat(x), nil
	case []byte:
		return NxBytes(x), nil
	case []any:
		out := make([]NexusValue, len(x))
		for i, e := range x {
			v, err := FromWire(e)
			if err != nil {
				return NexusValue{}, err
			}
			out[i] = v
		}
		return NxArray(out), nil
	case map[string]any:
		if len(x) != 1 {
			return NexusValue{}, fmt.Errorf(
				"decode: expected single-key tagged NexusValue, got %d keys", len(x),
			)
		}
		for tag, payload := range x {
			return fromWireTagged(tag, payload)
		}
	}
	return NexusValue{}, fmt.Errorf("decode: unexpected NexusValue wire type %T", raw)
}

func fromWireTagged(tag string, payload any) (NexusValue, error) {
	switch tag {
	case "Null":
		return NxNull(), nil
	case "Bool":
		b, ok := payload.(bool)
		if !ok {
			return NexusValue{}, fmt.Errorf("decode: Bool payload must be bool")
		}
		return NxBool(b), nil
	case "Int":
		switch v := payload.(type) {
		case int:
			return NxInt(int64(v)), nil
		case int8:
			return NxInt(int64(v)), nil
		case int16:
			return NxInt(int64(v)), nil
		case int32:
			return NxInt(int64(v)), nil
		case int64:
			return NxInt(v), nil
		case uint:
			return NxInt(int64(v)), nil
		case uint8:
			return NxInt(int64(v)), nil
		case uint16:
			return NxInt(int64(v)), nil
		case uint32:
			return NxInt(int64(v)), nil
		case uint64:
			return NxInt(int64(v)), nil
		}
		return NexusValue{}, fmt.Errorf("decode: Int payload must be integer, got %T", payload)
	case "Float":
		switch v := payload.(type) {
		case float32:
			return NxFloat(float64(v)), nil
		case float64:
			return NxFloat(v), nil
		case int64:
			return NxFloat(float64(v)), nil
		case int:
			return NxFloat(float64(v)), nil
		}
		return NexusValue{}, fmt.Errorf("decode: Float payload must be numeric, got %T", payload)
	case "Bytes":
		switch v := payload.(type) {
		case []byte:
			return NxBytes(v), nil
		case string:
			return NxBytes([]byte(v)), nil
		case []any:
			out := make([]byte, len(v))
			for i, e := range v {
				n, ok := e.(uint8)
				if !ok {
					return NexusValue{}, fmt.Errorf("decode: Bytes element must be uint8")
				}
				out[i] = n
			}
			return NxBytes(out), nil
		}
		return NexusValue{}, fmt.Errorf("decode: Bytes payload must be bytes, got %T", payload)
	case "Str":
		s, ok := payload.(string)
		if !ok {
			return NexusValue{}, fmt.Errorf("decode: Str payload must be string")
		}
		return NxStr(s), nil
	case "Array":
		arr, ok := payload.([]any)
		if !ok {
			return NexusValue{}, fmt.Errorf("decode: Array payload must be list")
		}
		out := make([]NexusValue, len(arr))
		for i, e := range arr {
			v, err := FromWire(e)
			if err != nil {
				return NexusValue{}, err
			}
			out[i] = v
		}
		return NxArray(out), nil
	case "Map":
		pairsRaw, ok := payload.([]any)
		if !ok {
			return NexusValue{}, fmt.Errorf("decode: Map payload must be list")
		}
		pairs := make([]MapEntry, len(pairsRaw))
		for i, p := range pairsRaw {
			kv, ok := p.([]any)
			if !ok || len(kv) != 2 {
				return NexusValue{}, fmt.Errorf("decode: Map entry must be [key, value] pair")
			}
			k, err := FromWire(kv[0])
			if err != nil {
				return NexusValue{}, err
			}
			v, err := FromWire(kv[1])
			if err != nil {
				return NexusValue{}, err
			}
			pairs[i] = MapEntry{Key: k, Value: v}
		}
		return NxMap(pairs), nil
	}
	return NexusValue{}, fmt.Errorf("decode: unknown NexusValue tag '%s'", tag)
}

// RpcRequest is the wire-level request frame.
type RpcRequest struct {
	ID      uint32
	Command string
	Args    []NexusValue
}

// RpcResponse is the decoded wire-level response.
type RpcResponse struct {
	ID  uint32
	OK  bool
	Val NexusValue // valid when OK=true
	Err string     // valid when OK=false
}

// Unwrap returns the response value or an error.
func (r RpcResponse) Unwrap() (NexusValue, error) {
	if !r.OK {
		return NexusValue{}, fmt.Errorf("server: %s", r.Err)
	}
	return r.Val, nil
}

// EncodeRequestFrame encodes a request into a length-prefixed
// MessagePack frame: `u32_le(body_len) ++ msgpack(body)`.
func EncodeRequestFrame(req RpcRequest) ([]byte, error) {
	args := make([]any, len(req.Args))
	for i, a := range req.Args {
		args[i] = ToWire(a)
	}
	body, err := msgpack.Marshal(map[string]any{
		"id":      req.ID,
		"command": req.Command,
		"args":    args,
	})
	if err != nil {
		return nil, err
	}
	out := make([]byte, 4+len(body))
	binary.LittleEndian.PutUint32(out[:4], uint32(len(body)))
	copy(out[4:], body)
	return out, nil
}

// DecodeResponseBody decodes a response body — the MessagePack bytes
// AFTER the length prefix. The caller is responsible for reading
// exactly `length` bytes off the wire before handing them here.
func DecodeResponseBody(body []byte) (RpcResponse, error) {
	dec := msgpack.NewDecoder(bytes.NewReader(body))
	dec.UseLooseInterfaceDecoding(true)
	var raw map[string]any
	if err := dec.Decode(&raw); err != nil {
		return RpcResponse{}, fmt.Errorf("decode: response must be a map: %w", err)
	}
	idRaw, ok := raw["id"]
	if !ok {
		return RpcResponse{}, fmt.Errorf("decode: response missing 'id'")
	}
	id, err := asUint32(idRaw)
	if err != nil {
		return RpcResponse{}, err
	}
	resultRaw, ok := raw["result"]
	if !ok {
		return RpcResponse{}, fmt.Errorf("decode: response missing 'result'")
	}
	resultMap, ok := resultRaw.(map[string]any)
	if !ok || len(resultMap) != 1 {
		return RpcResponse{}, fmt.Errorf("decode: Result must be a single-key tagged map")
	}
	for tag, payload := range resultMap {
		switch tag {
		case "Ok":
			val, err := FromWire(payload)
			if err != nil {
				return RpcResponse{}, err
			}
			return RpcResponse{ID: id, OK: true, Val: val}, nil
		case "Err":
			msg, _ := payload.(string)
			return RpcResponse{ID: id, OK: false, Err: msg}, nil
		default:
			return RpcResponse{}, fmt.Errorf("decode: Result must be 'Ok' or 'Err', got '%s'", tag)
		}
	}
	return RpcResponse{}, fmt.Errorf("decode: empty result map")
}

func asUint32(v any) (uint32, error) {
	switch n := v.(type) {
	case uint32:
		return n, nil
	case uint64:
		return uint32(n), nil
	case uint:
		return uint32(n), nil
	case int64:
		return uint32(n), nil
	case int:
		return uint32(n), nil
	case int32:
		return uint32(n), nil
	}
	return 0, fmt.Errorf("decode: 'id' must be integer, got %T", v)
}
