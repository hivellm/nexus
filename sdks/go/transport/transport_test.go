package transport

import (
	"context"
	"encoding/binary"
	"strings"
	"testing"
	"time"

	"github.com/vmihailenco/msgpack/v5"
)

// ── Endpoint parser ────────────────────────────────────────────────────

func TestEndpoint_DefaultLocalIsNexusLoopback(t *testing.T) {
	ep := DefaultLocalEndpoint()
	if ep.Scheme != "nexus" || ep.Host != "127.0.0.1" || ep.Port != RpcDefaultPort {
		t.Fatalf("unexpected default: %+v", ep)
	}
	if ep.String() != "nexus://127.0.0.1:15475" {
		t.Fatalf("bad string: %s", ep.String())
	}
}

func TestEndpoint_ParseNexusWithExplicitPort(t *testing.T) {
	ep, err := ParseEndpoint("nexus://example.com:17000")
	if err != nil {
		t.Fatal(err)
	}
	if ep.Scheme != "nexus" || ep.Port != 17000 {
		t.Fatalf("unexpected: %+v", ep)
	}
}

func TestEndpoint_ParseHttpDefaultPort(t *testing.T) {
	ep, err := ParseEndpoint("http://localhost")
	if err != nil {
		t.Fatal(err)
	}
	if ep.Scheme != "http" || ep.Port != HttpDefaultPort {
		t.Fatalf("unexpected: %+v", ep)
	}
}

func TestEndpoint_ParseHttpsDefaultPort(t *testing.T) {
	ep, err := ParseEndpoint("https://nexus.example.com")
	if err != nil {
		t.Fatal(err)
	}
	if ep.Scheme != "https" || ep.Port != HttpsDefaultPort {
		t.Fatalf("unexpected: %+v", ep)
	}
}

func TestEndpoint_ParseBareIsRpc(t *testing.T) {
	ep, err := ParseEndpoint("10.0.0.5:15600")
	if err != nil {
		t.Fatal(err)
	}
	if ep.Scheme != "nexus" || ep.Port != 15600 {
		t.Fatalf("unexpected: %+v", ep)
	}
}

func TestEndpoint_ParseIPv6(t *testing.T) {
	ep, err := ParseEndpoint("nexus://[::1]:15475")
	if err != nil {
		t.Fatal(err)
	}
	if ep.Host != "::1" || ep.Port != 15475 {
		t.Fatalf("unexpected: %+v", ep)
	}
}

func TestEndpoint_RejectsNexusRpcScheme(t *testing.T) {
	_, err := ParseEndpoint("nexus-rpc://host")
	if err == nil || !strings.Contains(err.Error(), "unsupported URL scheme") {
		t.Fatalf("expected rejection, got: %v", err)
	}
}

func TestEndpoint_RejectsEmpty(t *testing.T) {
	if _, err := ParseEndpoint(""); err == nil {
		t.Fatal("expected error")
	}
	if _, err := ParseEndpoint("   "); err == nil {
		t.Fatal("expected error")
	}
}

func TestEndpoint_AsHttpURLSwapsRpcToSiblingPort(t *testing.T) {
	ep, err := ParseEndpoint("nexus://host:17000")
	if err != nil {
		t.Fatal(err)
	}
	if ep.AsHttpURL() != "http://host:15474" {
		t.Fatalf("unexpected http url: %s", ep.AsHttpURL())
	}
}

// ── Wire codec ────────────────────────────────────────────────────────

func TestCodec_ToWireNull(t *testing.T) {
	v := ToWire(NxNull())
	if v != "Null" {
		t.Fatalf("expected 'Null', got %v", v)
	}
}

func TestCodec_ToWireStr(t *testing.T) {
	v := ToWire(NxStr("hi"))
	m, ok := v.(map[string]any)
	if !ok || m["Str"] != "hi" {
		t.Fatalf("expected {Str:hi}, got %v", v)
	}
}

func TestCodec_RoundtripPrimitives(t *testing.T) {
	cases := []NexusValue{
		NxNull(),
		NxBool(true),
		NxBool(false),
		NxInt(0),
		NxInt(-42),
		NxStr(""),
		NxStr("hello"),
		NxFloat(1.5),
		NxBytes([]byte{0, 255, 7}),
	}
	for _, in := range cases {
		back, err := FromWire(ToWire(in))
		if err != nil {
			t.Fatalf("from %+v: %v", in, err)
		}
		if back.Kind != in.Kind {
			t.Fatalf("roundtrip %+v -> %+v (kind mismatch)", in, back)
		}
	}
}

func TestCodec_RoundtripNestedArrayAndMap(t *testing.T) {
	v := NxMap([]MapEntry{
		{Key: NxStr("labels"), Value: NxArray([]NexusValue{NxStr("Person")})},
		{Key: NxStr("age"), Value: NxInt(30)},
	})
	back, err := FromWire(ToWire(v))
	if err != nil {
		t.Fatal(err)
	}
	if back.Kind != KindMap {
		t.Fatalf("expected Map, got %v", back.Kind)
	}
	pairs := back.Value.([]MapEntry)
	if len(pairs) != 2 {
		t.Fatalf("expected 2 pairs, got %d", len(pairs))
	}
}

func TestCodec_RejectsMultiKeyTaggedValue(t *testing.T) {
	_, err := FromWire(map[string]any{"Str": "a", "Int": int64(1)})
	if err == nil || !strings.Contains(err.Error(), "single-key") {
		t.Fatalf("expected 'single-key' error, got: %v", err)
	}
}

func TestCodec_RejectsUnknownTag(t *testing.T) {
	_, err := FromWire(map[string]any{"Widget": "x"})
	if err == nil || !strings.Contains(err.Error(), "unknown NexusValue tag") {
		t.Fatalf("expected unknown-tag error, got: %v", err)
	}
}

func TestCodec_RequestFrameHasU32LELengthPrefix(t *testing.T) {
	frame, err := EncodeRequestFrame(RpcRequest{ID: 7, Command: "PING"})
	if err != nil {
		t.Fatal(err)
	}
	length := binary.LittleEndian.Uint32(frame[:4])
	if int(length) != len(frame)-4 {
		t.Fatalf("length prefix mismatch: %d vs %d", length, len(frame)-4)
	}
	if length == 0 {
		t.Fatal("body must be non-empty")
	}
}

func TestCodec_DecodeOkResponse(t *testing.T) {
	body, err := msgpack.Marshal(map[string]any{
		"id":     uint32(9),
		"result": map[string]any{"Ok": map[string]any{"Str": "OK"}},
	})
	if err != nil {
		t.Fatal(err)
	}
	resp, err := DecodeResponseBody(body)
	if err != nil {
		t.Fatal(err)
	}
	if resp.ID != 9 || !resp.OK {
		t.Fatalf("unexpected: %+v", resp)
	}
	val, err := resp.Unwrap()
	if err != nil {
		t.Fatal(err)
	}
	if s, _ := val.AsString(); s != "OK" {
		t.Fatalf("expected OK, got %v", val)
	}
}

func TestCodec_DecodeErrResponse(t *testing.T) {
	body, err := msgpack.Marshal(map[string]any{
		"id":     uint32(3),
		"result": map[string]any{"Err": "boom"},
	})
	if err != nil {
		t.Fatal(err)
	}
	resp, err := DecodeResponseBody(body)
	if err != nil {
		t.Fatal(err)
	}
	if resp.OK || resp.Err != "boom" {
		t.Fatalf("unexpected: %+v", resp)
	}
	if _, err := resp.Unwrap(); err == nil || !strings.Contains(err.Error(), "boom") {
		t.Fatalf("unwrap: %v", err)
	}
}

// ── Command map ───────────────────────────────────────────────────────

func TestCommandMap_CypherSimple(t *testing.T) {
	m := MapCommand("graph.cypher", map[string]any{"query": "RETURN 1"})
	if m == nil || m.Command != "CYPHER" {
		t.Fatalf("unexpected: %+v", m)
	}
	if s, _ := m.Args[0].AsString(); s != "RETURN 1" {
		t.Fatalf("wrong arg: %+v", m.Args[0])
	}
}

func TestCommandMap_CypherWithParams(t *testing.T) {
	m := MapCommand("graph.cypher", map[string]any{
		"query":      "MATCH (n {name:$n}) RETURN n",
		"parameters": map[string]any{"n": "Alice"},
	})
	if m == nil || len(m.Args) != 2 {
		t.Fatalf("unexpected: %+v", m)
	}
	if m.Args[1].Kind != KindMap {
		t.Fatalf("expected Map, got %v", m.Args[1].Kind)
	}
}

func TestCommandMap_NoArgVerbs(t *testing.T) {
	for _, name := range []string{"graph.ping", "graph.stats", "graph.health", "graph.quit"} {
		m := MapCommand(name, map[string]any{})
		if m == nil || len(m.Args) != 0 {
			t.Fatalf("%s unexpected: %+v", name, m)
		}
	}
}

func TestCommandMap_AuthApiKeyWins(t *testing.T) {
	m := MapCommand("auth.login", map[string]any{"api_key": "nx_1", "username": "u", "password": "p"})
	if m == nil || len(m.Args) != 1 {
		t.Fatalf("unexpected: %+v", m)
	}
}

func TestCommandMap_AuthFallsBack(t *testing.T) {
	m := MapCommand("auth.login", map[string]any{"username": "u", "password": "p"})
	if m == nil || len(m.Args) != 2 {
		t.Fatalf("unexpected: %+v", m)
	}
}

func TestCommandMap_DbCreateRequiresName(t *testing.T) {
	if MapCommand("db.create", map[string]any{}) != nil {
		t.Fatal("expected nil")
	}
	m := MapCommand("db.create", map[string]any{"name": "mydb"})
	if m == nil || m.Command != "DB_CREATE" {
		t.Fatalf("unexpected: %+v", m)
	}
}

func TestCommandMap_Unknown(t *testing.T) {
	if MapCommand("graph.nonsense", map[string]any{}) != nil {
		t.Fatal("expected nil")
	}
}

// ── TransportMode parse ───────────────────────────────────────────────

func TestParseMode_CanonicalTokens(t *testing.T) {
	cases := map[string]Mode{"nexus": ModeNexusRpc, "http": ModeHttp, "https": ModeHttps, "resp3": ModeResp3}
	for in, want := range cases {
		got, ok := ParseMode(in)
		if !ok || got != want {
			t.Fatalf("ParseMode(%q) = (%v, %v), want (%v, true)", in, got, ok, want)
		}
	}
}

func TestParseMode_Aliases(t *testing.T) {
	for _, in := range []string{"rpc", "NexusRpc", "NEXUSRPC"} {
		got, ok := ParseMode(in)
		if !ok || got != ModeNexusRpc {
			t.Fatalf("alias %q: (%v, %v)", in, got, ok)
		}
	}
}

func TestParseMode_EmptyAndAuto(t *testing.T) {
	for _, in := range []string{"", "auto", "widget"} {
		if _, ok := ParseMode(in); ok {
			t.Fatalf("ParseMode(%q) should return ok=false", in)
		}
	}
}

// ── Build precedence ──────────────────────────────────────────────────

func TestBuild_DefaultIsRpc(t *testing.T) {
	built, err := Build(BuildOptions{}, Credentials{})
	if err != nil {
		t.Fatal(err)
	}
	if built.Mode != ModeNexusRpc || built.Endpoint.Port != RpcDefaultPort {
		t.Fatalf("unexpected: %+v", built)
	}
}

func TestBuild_URLSchemeWinsOverEnv(t *testing.T) {
	built, err := Build(BuildOptions{BaseURL: "http://host:15474", EnvTransport: "nexus"}, Credentials{})
	if err != nil {
		t.Fatal(err)
	}
	if built.Mode != ModeHttp {
		t.Fatalf("expected HTTP, got %s", built.Mode)
	}
}

func TestBuild_EnvOverridesBareHost(t *testing.T) {
	built, err := Build(BuildOptions{BaseURL: "host:15474", EnvTransport: "http"}, Credentials{})
	if err != nil {
		t.Fatal(err)
	}
	if built.Mode != ModeHttp {
		t.Fatalf("expected HTTP, got %s", built.Mode)
	}
}

func TestBuild_Resp3RaisesClearError(t *testing.T) {
	_, err := Build(BuildOptions{Transport: ModeResp3}, Credentials{})
	if err == nil || !strings.Contains(err.Error(), "resp3 transport is not yet shipped") {
		t.Fatalf("expected RESP3 error, got: %v", err)
	}
}

// ── Credentials ───────────────────────────────────────────────────────

func TestCredentials_HasAny(t *testing.T) {
	if (Credentials{}).HasAny() {
		t.Fatal("empty should have none")
	}
	if !(Credentials{APIKey: "k"}).HasAny() {
		t.Fatal("api key should count")
	}
	if (Credentials{Username: "u"}).HasAny() {
		t.Fatal("username alone should not count")
	}
	if !(Credentials{Username: "u", Password: "p"}).HasAny() {
		t.Fatal("user+pass should count")
	}
}

// ── RPC fails fast on connect refused ─────────────────────────────────

func TestRpcTransport_FailsFastOnUnreachableHost(t *testing.T) {
	t.Parallel()
	ep := Endpoint{Scheme: "nexus", Host: "127.0.0.1", Port: 1} // reserved
	tr := NewRpcTransport(ep, Credentials{})
	tr.SetConnectTimeout(500 * time.Millisecond)
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	_, err := tr.Call(ctx, "PING", nil)
	if err == nil {
		t.Fatal("expected connection error")
	}
	if !strings.Contains(err.Error(), "failed to connect") &&
		!strings.Contains(err.Error(), "refused") &&
		!strings.Contains(err.Error(), "connection") {
		t.Fatalf("expected connect failure, got: %v", err)
	}
}
