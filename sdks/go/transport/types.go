// Package transport is the wire-format layer for the Nexus Go SDK.
//
// Every [github.com/hivellm/nexus-go.Client] delegates its wire format
// to a [Transport] picked at construction time. Three modes are
// recognised:
//
//   - "nexus" — native binary RPC (length-prefixed MessagePack on port
//     15475). **Default.**
//   - "http" / "https" — JSON over REST (port 15474 / 443). Legacy /
//     firewall-friendly.
//   - "resp3" — reserved for a future RESP3 implementation.
//
// Precedence for picking the transport:
//
//  1. URL scheme in baseURL (`nexus://` → RPC, `http://` → HTTP, …)
//  2. NEXUS_SDK_TRANSPORT env var
//  3. Config.Transport field
//  4. Default: "nexus"
//
// See docs/specs/sdk-transport.md for the cross-SDK contract.
package transport

import (
	"context"
	"fmt"
	"strings"
)

// Mode selects the wire transport. Values match the URL-scheme tokens
// and the NEXUS_SDK_TRANSPORT env-var strings.
type Mode string

const (
	// ModeNexusRpc is the native binary RPC transport on port 15475.
	ModeNexusRpc Mode = "nexus"
	// ModeResp3 is the RESP3 transport on port 15476.
	ModeResp3 Mode = "resp3"
	// ModeHttp is the plain-text JSON/HTTP transport on port 15474.
	ModeHttp Mode = "http"
	// ModeHttps is the TLS HTTPS transport on port 443.
	ModeHttps Mode = "https"
)

// ParseMode parses the NEXUS_SDK_TRANSPORT env-var token (or any
// caller-stashed mode string). Accepts the canonical values plus the
// "rpc" / "nexusrpc" aliases for ergonomics. Returns an empty mode and
// nil error for "" or "auto".
func ParseMode(raw string) (Mode, bool) {
	v := strings.ToLower(strings.TrimSpace(raw))
	switch v {
	case "nexus", "rpc", "nexusrpc":
		return ModeNexusRpc, true
	case "resp3":
		return ModeResp3, true
	case "http":
		return ModeHttp, true
	case "https":
		return ModeHttps, true
	case "", "auto":
		return "", false
	}
	return "", false
}

// IsRpc reports whether the mode carries the native binary RPC wire format.
func (m Mode) IsRpc() bool { return m == ModeNexusRpc }

// Kind is the discriminator for a [NexusValue]. The tagged-union shape
// mirrors sdks/rust/src/transport/... and sdks/python/nexus_sdk/transport/types.py.
type Kind string

const (
	KindNull  Kind = "Null"
	KindBool  Kind = "Bool"
	KindInt   Kind = "Int"
	KindFloat Kind = "Float"
	KindBytes Kind = "Bytes"
	KindStr   Kind = "Str"
	KindArray Kind = "Array"
	KindMap   Kind = "Map"
)

// NexusValue is a dynamically-typed value carried by RPC requests and
// responses. Use the Nx* constructors to build values at call sites.
type NexusValue struct {
	Kind  Kind
	Value any
}

// MapEntry is a single pair inside a [NexusValue] of [KindMap]. Keys
// can be non-string NexusValues, matching the Rust server's shape.
type MapEntry struct {
	Key, Value NexusValue
}

// Nx* — constructors for NexusValue variants.
func NxNull() NexusValue                 { return NexusValue{Kind: KindNull} }
func NxBool(b bool) NexusValue           { return NexusValue{Kind: KindBool, Value: b} }
func NxInt(i int64) NexusValue           { return NexusValue{Kind: KindInt, Value: i} }
func NxFloat(f float64) NexusValue       { return NexusValue{Kind: KindFloat, Value: f} }
func NxBytes(b []byte) NexusValue        { return NexusValue{Kind: KindBytes, Value: b} }
func NxStr(s string) NexusValue          { return NexusValue{Kind: KindStr, Value: s} }
func NxArray(a []NexusValue) NexusValue  { return NexusValue{Kind: KindArray, Value: a} }
func NxMap(pairs []MapEntry) NexusValue  { return NexusValue{Kind: KindMap, Value: pairs} }

// AsString returns the inner string if Kind == KindStr, else "" and ok=false.
func (v NexusValue) AsString() (string, bool) {
	if v.Kind == KindStr {
		if s, ok := v.Value.(string); ok {
			return s, true
		}
	}
	return "", false
}

// Credentials carried by a transport. Both paths may be set; APIKey wins.
type Credentials struct {
	APIKey   string
	Username string
	Password string
}

// HasAny reports whether any credential is set.
func (c Credentials) HasAny() bool {
	return c.APIKey != "" || (c.Username != "" && c.Password != "")
}

// Request is a single request against the active transport.
type Request struct {
	// Command is the wire-level verb ("CYPHER", "PING", "STATS", …).
	Command string
	// Args is the positional argument vector.
	Args []NexusValue
}

// Response is a single response from the active transport.
type Response struct {
	Value NexusValue
}

// Transport is the generic transport interface — one method per
// request/response pair.
type Transport interface {
	Execute(ctx context.Context, req Request) (Response, error)
	Describe() string
	IsRpc() bool
	Close() error
}

// ErrUnmappedCommand is returned by the HTTP transport when a caller
// routes a wire verb the HTTP route table does not understand.
type ErrUnmappedCommand struct{ Command string }

func (e *ErrUnmappedCommand) Error() string {
	return fmt.Sprintf(
		"HTTP fallback does not know how to route '%s' — add an entry to sdks/go/transport/http.go",
		e.Command,
	)
}

// HttpError wraps a non-2xx HTTP response so callers can recover the
// status code + body without parsing error strings.
type HttpError struct {
	StatusCode int
	Body       string
}

func (e *HttpError) Error() string {
	return fmt.Sprintf("HTTP %d: %s", e.StatusCode, e.Body)
}
