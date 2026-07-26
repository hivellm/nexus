// Interop cell: Go SDK (github.com/hivellm/nexus-go) against a Thunder-based
// server.
//
// Drives transport.RpcTransport directly rather than the sugar client — the
// matrix is about the wire, and the transport is where the wire lives.
//
//	argv:   <host> <port> <user> <pass>
//	stdout: one `STEP <name> PASS|FAIL <detail>` line per step
//	exit:   0 iff every step passed
package main

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"math"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/hivellm/nexus-go/transport"
)

// vecBytes is the f32-LE encoding of [1.5, -2.5, 3.5, +Inf] — emphatically
// not valid UTF-8, so a transport that quietly round-trips Bytes through a
// string cannot pass the knn_bytes step.
var vec = []float32{1.5, -2.5, 3.5, float32(math.Inf(1))}
var vecBytes = f32LEBytes(vec)

// idMarker is the node id used by the cypher step.
const idMarker int64 = 424203

func f32LEBytes(values []float32) []byte {
	buf := make([]byte, 4*len(values))
	for i, v := range values {
		binary.LittleEndian.PutUint32(buf[i*4:], math.Float32bits(v))
	}
	return buf
}

func report(step string, ok bool, detail string) {
	status := "FAIL"
	if ok {
		status = "PASS"
	}
	fmt.Printf("STEP %s %s %s\n", step, status, detail)
}

// mapGet looks up a string key in a Map-kind NexusValue.
func mapGet(v transport.NexusValue, key string) (transport.NexusValue, bool) {
	if v.Kind != transport.KindMap {
		return transport.NexusValue{}, false
	}
	pairs, ok := v.Value.([]transport.MapEntry)
	if !ok {
		return transport.NexusValue{}, false
	}
	for _, p := range pairs {
		if s, ok := p.Key.AsString(); ok && s == key {
			return p.Value, true
		}
	}
	return transport.NexusValue{}, false
}

// cypherRows runs CYPHER over the transport and returns rows as
// [][]transport.NexusValue, mirroring the reference cell's cypher_rows.
func cypherRows(ctx context.Context, t transport.Transport, query string, params []transport.MapEntry) ([][]transport.NexusValue, error) {
	args := []transport.NexusValue{transport.NxStr(query)}
	if len(params) > 0 {
		args = append(args, transport.NxMap(params))
	}
	resp, err := t.Execute(ctx, transport.Request{Command: "CYPHER", Args: args})
	if err != nil {
		return nil, err
	}
	if errVal, ok := mapGet(resp.Value, "error"); ok {
		if s, ok := errVal.AsString(); ok && s != "" {
			return nil, fmt.Errorf("%s", s)
		}
	}
	rowsVal, ok := mapGet(resp.Value, "rows")
	if !ok || rowsVal.Kind != transport.KindArray {
		return nil, nil
	}
	rowsArr, _ := rowsVal.Value.([]transport.NexusValue)
	out := make([][]transport.NexusValue, 0, len(rowsArr))
	for _, row := range rowsArr {
		if row.Kind != transport.KindArray {
			continue
		}
		cells, _ := row.Value.([]transport.NexusValue)
		out = append(out, cells)
	}
	return out, nil
}

func isPong(v transport.NexusValue) bool {
	s, ok := v.AsString()
	return ok && s == "PONG"
}

// stepAuth proves PING answers before AUTH, STATS is refused (typed NOAUTH)
// before AUTH, and STATS succeeds after AUTH. Returns the authenticated
// transport for subsequent steps regardless of the outcome, so the matrix
// still gets all four STEP lines.
func stepAuth(ctx context.Context, endpoint transport.Endpoint, user, pass string) (bool, string, *transport.RpcTransport) {
	anon := transport.NewRpcTransport(endpoint, transport.Credentials{})
	pingResp, pingErr := anon.Execute(ctx, transport.Request{Command: "PING"})
	pingOK := pingErr == nil && isPong(pingResp.Value)

	_, statsErr := anon.Execute(ctx, transport.Request{Command: "STATS"})
	statsPreRefused := false
	if statsErr != nil {
		msg := statsErr.Error()
		statsPreRefused = strings.Contains(strings.ToLower(msg), "auth") || strings.Contains(msg, "NOAUTH")
	}
	if err := anon.Close(); err != nil {
		fmt.Fprintf(os.Stderr, "warning: closing anonymous transport: %v\n", err)
	}

	authed := transport.NewRpcTransport(endpoint, transport.Credentials{Username: user, Password: pass})
	statsResp, statsErr2 := authed.Execute(ctx, transport.Request{Command: "STATS"})
	statsPostOK := statsErr2 == nil && (statsResp.Value.Kind == transport.KindMap || statsResp.Value.Kind == transport.KindStr)

	ok := pingOK && statsPreRefused && statsPostOK
	detail := fmt.Sprintf("ping=%v stats_pre_refused=%v stats_post=%v", pingOK, statsPreRefused, statsPostOK)
	return ok, detail, authed
}

// stepCypher proves CREATE then MATCH round-trips the id parameter back.
func stepCypher(ctx context.Context, t transport.Transport) (bool, string) {
	params := []transport.MapEntry{{Key: transport.NxStr("id"), Value: transport.NxInt(idMarker)}}

	if _, err := cypherRows(ctx, t, "CREATE (n:InteropGo {id: $id}) RETURN n.id", params); err != nil {
		return false, fmt.Sprintf("%T: %v", err, err)
	}
	rows, err := cypherRows(ctx, t, "MATCH (n:InteropGo {id: $id}) RETURN n.id", params)
	if err != nil {
		return false, fmt.Sprintf("%T: %v", err, err)
	}

	var got any
	ok := false
	if len(rows) > 0 && len(rows[0]) > 0 {
		cell := rows[0][0]
		got = cell.Value
		if cell.Kind == transport.KindInt {
			if i, isInt := cell.Value.(int64); isInt && i == idMarker {
				ok = true
			}
		}
	}
	return ok, fmt.Sprintf("round-trip id -> %v", got)
}

// stepKnnBytes proves a raw f32-LE vector carried as Bytes round-trips
// byte-for-byte via a PING echo.
func stepKnnBytes(ctx context.Context, t transport.Transport) (bool, string) {
	resp, err := t.Execute(ctx, transport.Request{
		Command: "PING",
		Args:    []transport.NexusValue{transport.NxBytes(vecBytes)},
	})
	if err != nil {
		return false, fmt.Sprintf("%s -> error: %v", hex.EncodeToString(vecBytes), err)
	}
	var got []byte
	if resp.Value.Kind == transport.KindBytes {
		got, _ = resp.Value.Value.([]byte)
	}
	ok := bytes.Equal(got, vecBytes) && len(vecBytes) == 4*len(vec)
	return ok, fmt.Sprintf("%s -> %s", hex.EncodeToString(vecBytes), hex.EncodeToString(got))
}

// stepError proves a broken CYPHER surfaces a typed server error (not a
// transport crash) and the same connection stays usable afterwards.
func stepError(ctx context.Context, t transport.Transport) (bool, string) {
	_, err := cypherRows(ctx, t, "MATCH (n RETURN", nil)
	if err == nil {
		return false, "expected a server error, got a result"
	}

	pingResp, pingErr := t.Execute(ctx, transport.Request{Command: "PING"})
	alive := pingErr == nil && isPong(pingResp.Value)
	return alive, fmt.Sprintf("raised %T; connection alive=%v", err, alive)
}

func main() {
	if len(os.Args) < 5 {
		fmt.Fprintln(os.Stderr, "usage: main <host> <port> <user> <pass>")
		os.Exit(1)
	}
	host := os.Args[1]
	port, err := strconv.ParseUint(os.Args[2], 10, 16)
	if err != nil {
		fmt.Fprintf(os.Stderr, "invalid port %q: %v\n", os.Args[2], err)
		os.Exit(1)
	}
	user, pass := os.Args[3], os.Args[4]

	endpoint := transport.Endpoint{Scheme: "nexus", Host: host, Port: uint16(port)}
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	failures := 0

	authOK, authDetail, authed := stepAuth(ctx, endpoint, user, pass)
	report("auth", authOK, authDetail)
	if !authOK {
		failures++
	}

	cypherOK, cypherDetail := stepCypher(ctx, authed)
	report("cypher", cypherOK, cypherDetail)
	if !cypherOK {
		failures++
	}

	bytesOK, bytesDetail := stepKnnBytes(ctx, authed)
	report("knn_bytes", bytesOK, bytesDetail)
	if !bytesOK {
		failures++
	}

	errOK, errDetail := stepError(ctx, authed)
	report("error", errOK, errDetail)
	if !errOK {
		failures++
	}

	if err := authed.Close(); err != nil {
		fmt.Fprintf(os.Stderr, "warning: closing authenticated transport: %v\n", err)
	}

	if failures > 0 {
		os.Exit(1)
	}
	os.Exit(0)
}
