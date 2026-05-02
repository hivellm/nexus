//go:build live
// +build live

package main

import (
	"context"
	"fmt"
	"math/rand"
	"os"
	"strings"
	"testing"
	"time"

	nexus "github.com/hivellm/nexus-go"
)

// liveClient builds an HTTP client pointed at NEXUS_LIVE_HOST.
// Each test that calls this helper skips itself when the env var is unset.
func liveClient(t *testing.T) *nexus.Client {
	t.Helper()
	host := os.Getenv("NEXUS_LIVE_HOST")
	if host == "" {
		t.Skip("NEXUS_LIVE_HOST not set — skipping live test")
	}
	c, err := nexus.NewClientE(nexus.Config{
		BaseURL: host,
		Timeout: 30 * time.Second,
	})
	if err != nil {
		t.Fatalf("failed to build client: %v", err)
	}
	return c
}

// uniqueID returns a random 16-character hex token used to isolate test data
// across runs so each invocation operates on fresh external ids.
func uniqueID() string {
	src := rand.NewSource(time.Now().UnixNano())
	r := rand.New(src)
	return fmt.Sprintf("%016x", r.Int63())
}

// createNodeMustSucceed calls CreateNodeWithExternalID and fails the test
// immediately when the server-level error field is populated.  The server
// returns HTTP 200 with {"node_id":0,"error":"..."} for validation failures
// rather than a 4xx status code, so callers must inspect resp.Error directly.
func createNodeMustSucceed(
	t *testing.T,
	client *nexus.Client,
	labels []string,
	props map[string]interface{},
	extID, policy string,
) *nexus.CreateNodeResponse {
	t.Helper()
	resp, err := client.CreateNodeWithExternalID(context.Background(), labels, props, extID, policy)
	if err != nil {
		t.Fatalf("CreateNodeWithExternalID HTTP error for %q: %v", extID, err)
	}
	if resp.Error != nil {
		t.Fatalf("CreateNodeWithExternalID server error for %q: %s", extID, *resp.Error)
	}
	if resp.NodeID == 0 {
		t.Fatalf("CreateNodeWithExternalID returned node_id=0 for %q (message: %q)", extID, resp.Message)
	}
	return resp
}

// createNodeMustFail asserts that the server rejects the request.  The server
// signals rejection either as a non-nil HTTP error (4xx/5xx) or as HTTP 200
// with a populated error field and node_id=0, so both paths are checked.
func createNodeMustFail(
	t *testing.T,
	client *nexus.Client,
	labels []string,
	props map[string]interface{},
	extID, policy string,
) {
	t.Helper()
	resp, err := client.CreateNodeWithExternalID(context.Background(), labels, props, extID, policy)
	if err != nil {
		// HTTP-level error — rejection confirmed.
		return
	}
	if resp.Error != nil {
		// Server returned 200 with an error field — rejection confirmed.
		return
	}
	t.Fatalf("expected rejection for external_id=%q but server accepted it (node_id=%d)", extID, resp.NodeID)
}

// getNodeByExtID wraps GetNodeByExternalID and handles the Node.ID type
// mismatch: the SDK struct declares ID as string but the server sends an
// integer.  When the JSON decoder fails with that mismatch the helper falls
// back to a Cypher probe so the round-trip is still validated.
// Returns (internalID interface{}, found bool).
func getNodeByExtID(t *testing.T, client *nexus.Client, extID string) (interface{}, bool) {
	t.Helper()
	resp, err := client.GetNodeByExternalID(context.Background(), extID)
	if err != nil {
		if strings.Contains(err.Error(), "cannot unmarshal") {
			// The server found the node but the SDK cannot decode the integer
			// id field.  Confirm existence via Cypher.
			qr, qerr := client.ExecuteCypher(
				context.Background(),
				fmt.Sprintf("MATCH (n {_id: '%s'}) RETURN id(n)", extID),
				nil,
			)
			if qerr != nil || len(qr.Rows) == 0 || len(qr.Rows[0]) == 0 {
				return nil, false
			}
			return qr.Rows[0][0], true
		}
		t.Fatalf("GetNodeByExternalID network error for %q: %v", extID, err)
	}
	if resp.Node == nil {
		return nil, false
	}
	return resp.Node.ID, true
}

// TestLive_HealthCheck verifies the server is reachable before the rest of the
// suite runs.
func TestLive_HealthCheck(t *testing.T) {
	client := liveClient(t)
	if err := client.Ping(context.Background()); err != nil {
		t.Fatalf("server not reachable at %s: %v", os.Getenv("NEXUS_LIVE_HOST"), err)
	}
}

// TestLive_ExternalID_Sha256 tests the sha256 variant create and round-trip GET.
func TestLive_ExternalID_Sha256(t *testing.T) {
	client := liveClient(t)
	uid := uniqueID() // 16 hex chars
	// sha256 hex must be exactly 64 chars.
	extID := "sha256:" + strings.Repeat("1", 48) + uid

	resp := createNodeMustSucceed(t, client, []string{"LiveSha256"}, map[string]interface{}{"uid": uid}, extID, "")

	_, found := getNodeByExtID(t, client, extID)
	if !found {
		t.Fatalf("GET by sha256 external id returned not-found (created node_id=%d)", resp.NodeID)
	}
}

// TestLive_ExternalID_Blake3 tests the blake3 variant create and round-trip GET.
func TestLive_ExternalID_Blake3(t *testing.T) {
	client := liveClient(t)
	uid := uniqueID()
	// blake3 hex must be exactly 64 chars.
	extID := "blake3:" + strings.Repeat("2", 48) + uid

	resp := createNodeMustSucceed(t, client, []string{"LiveBlake3"}, map[string]interface{}{"uid": uid}, extID, "")

	_, found := getNodeByExtID(t, client, extID)
	if !found {
		t.Fatalf("GET by blake3 external id returned not-found (created node_id=%d)", resp.NodeID)
	}
}

// TestLive_ExternalID_Sha512 tests the sha512 variant create and round-trip GET.
func TestLive_ExternalID_Sha512(t *testing.T) {
	client := liveClient(t)
	uid := uniqueID()
	// sha512 hex must be exactly 128 chars.
	extID := "sha512:" + strings.Repeat("3", 112) + uid

	resp := createNodeMustSucceed(t, client, []string{"LiveSha512"}, map[string]interface{}{"uid": uid}, extID, "")

	_, found := getNodeByExtID(t, client, extID)
	if !found {
		t.Fatalf("GET by sha512 external id returned not-found (created node_id=%d)", resp.NodeID)
	}
}

// TestLive_ExternalID_UUID tests the uuid variant create and round-trip GET.
// A canonical UUID is 8-4-4-4-12 lowercase hex digits (32 hex total).
func TestLive_ExternalID_UUID(t *testing.T) {
	client := liveClient(t)
	uid := uniqueID() // 16 hex chars
	hex32 := uid + uid // 32 hex chars
	extID := fmt.Sprintf("uuid:%s-%s-%s-%s-%s",
		hex32[0:8],
		hex32[8:12],
		"4"+hex32[13:16], // version nibble forced to 4
		"8"+hex32[17:20], // variant nibble forced to 8 (4 chars total)
		hex32[20:32],
	)

	resp := createNodeMustSucceed(t, client, []string{"LiveUUID"}, map[string]interface{}{"uid": uid}, extID, "")

	_, found := getNodeByExtID(t, client, extID)
	if !found {
		t.Fatalf("GET by uuid external id returned not-found (created node_id=%d)", resp.NodeID)
	}
}

// TestLive_ExternalID_Str tests the str variant create and round-trip GET.
func TestLive_ExternalID_Str(t *testing.T) {
	client := liveClient(t)
	uid := uniqueID()
	extID := "str:live-go-sdk-str-" + uid

	resp := createNodeMustSucceed(t, client, []string{"LiveStr"}, map[string]interface{}{"uid": uid}, extID, "")

	_, found := getNodeByExtID(t, client, extID)
	if !found {
		t.Fatalf("GET by str external id returned not-found (created node_id=%d)", resp.NodeID)
	}
}

// TestLive_ExternalID_Bytes tests the bytes variant create and round-trip GET.
func TestLive_ExternalID_Bytes(t *testing.T) {
	client := liveClient(t)
	uid := uniqueID()
	// bytes payload is hex; uid is 16 hex chars (8 bytes) — well within the 64-byte cap.
	extID := "bytes:" + uid

	resp := createNodeMustSucceed(t, client, []string{"LiveBytes"}, map[string]interface{}{"uid": uid}, extID, "")

	_, found := getNodeByExtID(t, client, extID)
	if !found {
		t.Fatalf("GET by bytes external id returned not-found (created node_id=%d)", resp.NodeID)
	}
}

// TestLive_ConflictPolicy_Error verifies that a second create with
// conflict_policy=error is rejected.
func TestLive_ConflictPolicy_Error(t *testing.T) {
	client := liveClient(t)
	uid := uniqueID()
	extID := "str:live-go-conflict-error-" + uid

	createNodeMustSucceed(t, client,
		[]string{"LiveConflictError"}, map[string]interface{}{"attempt": "first"},
		extID, "error")

	createNodeMustFail(t, client,
		[]string{"LiveConflictError"}, map[string]interface{}{"attempt": "second"},
		extID, "error")
}

// TestLive_ConflictPolicy_Match verifies that conflict_policy=match returns the
// existing node id rather than creating a new one.
func TestLive_ConflictPolicy_Match(t *testing.T) {
	client := liveClient(t)
	uid := uniqueID()
	extID := "str:live-go-conflict-match-" + uid

	first := createNodeMustSucceed(t, client,
		[]string{"LiveConflictMatch"}, map[string]interface{}{"value": "original"},
		extID, "error")

	second := createNodeMustSucceed(t, client,
		[]string{"LiveConflictMatch"}, map[string]interface{}{"value": "ignored"},
		extID, "match")

	if second.NodeID != first.NodeID {
		t.Fatalf("conflict_policy=match should return existing node_id %d, got %d",
			first.NodeID, second.NodeID)
	}
}

// TestLive_ConflictPolicy_Replace verifies that conflict_policy=replace
// overwrites node properties and returns the same node id.
// This is a regression guard for commit fd001344 (REPLACE prop-ptr fix).
func TestLive_ConflictPolicy_Replace(t *testing.T) {
	client := liveClient(t)
	uid := uniqueID()
	extID := "str:live-go-conflict-replace-" + uid

	first := createNodeMustSucceed(t, client,
		[]string{"LiveConflictReplace"}, map[string]interface{}{"status": "original"},
		extID, "error")

	second := createNodeMustSucceed(t, client,
		[]string{"LiveConflictReplace"}, map[string]interface{}{"status": "replaced"},
		extID, "replace")

	if second.NodeID != first.NodeID {
		t.Fatalf("conflict_policy=replace should return same node_id %d, got %d",
			first.NodeID, second.NodeID)
	}

	// Verify the property actually changed — this is the fd001344 regression check.
	// Use Cypher to read back the property, avoiding the Node.ID decode mismatch.
	qr, err := client.ExecuteCypher(
		context.Background(),
		fmt.Sprintf("MATCH (n {_id: '%s'}) RETURN n.status", extID),
		nil,
	)
	if err != nil {
		t.Fatalf("Cypher probe after replace failed: %v", err)
	}
	if len(qr.Rows) == 0 || len(qr.Rows[0]) == 0 {
		t.Fatal("Cypher probe returned no rows — node not found after replace")
	}
	got := fmt.Sprintf("%v", qr.Rows[0][0])
	if got != "replaced" {
		t.Fatalf("property 'status' should be 'replaced' after conflict_policy=replace, got %q", got)
	}
}

// TestLive_CypherIDRoundTrip tests CREATE (n:T {_id: '...'}) RETURN n._id via
// ExecuteCypher and asserts the first cell equals the prefixed-string form.
func TestLive_CypherIDRoundTrip(t *testing.T) {
	client := liveClient(t)
	uid := uniqueID()
	extID := "str:live-go-cypher-id-" + uid

	result, err := client.ExecuteCypher(
		context.Background(),
		fmt.Sprintf("CREATE (n:LiveCypherTest {_id: '%s', tag: 'go-live'}) RETURN n._id", extID),
		nil,
	)
	if err != nil {
		t.Fatalf("ExecuteCypher CREATE with _id failed: %v", err)
	}
	if len(result.Rows) == 0 || len(result.Rows[0]) == 0 {
		t.Fatal("expected at least one row from CREATE RETURN n._id")
	}
	got := fmt.Sprintf("%v", result.Rows[0][0])
	if got != extID {
		t.Fatalf("n._id should equal %q, got %q", extID, got)
	}
}

// TestLive_LengthCap_StrTooLong verifies that a str payload > 256 bytes is
// rejected.  The server returns HTTP 200 with a populated error field and
// node_id=0 rather than a 4xx.
func TestLive_LengthCap_StrTooLong(t *testing.T) {
	client := liveClient(t)
	extID := "str:" + strings.Repeat("a", 257)
	createNodeMustFail(t, client, []string{"LiveLengthCap"}, map[string]interface{}{}, extID, "")
}

// TestLive_LengthCap_BytesTooLong verifies that a bytes payload > 64 bytes is
// rejected.  65 bytes expressed as 130 hex chars.
func TestLive_LengthCap_BytesTooLong(t *testing.T) {
	client := liveClient(t)
	extID := "bytes:" + strings.Repeat("ff", 65)
	createNodeMustFail(t, client, []string{"LiveLengthCap"}, map[string]interface{}{}, extID, "")
}

// TestLive_LengthCap_EmptyUUID verifies that an empty uuid payload is rejected.
func TestLive_LengthCap_EmptyUUID(t *testing.T) {
	client := liveClient(t)
	createNodeMustFail(t, client, []string{"LiveLengthCap"}, map[string]interface{}{}, "uuid:", "")
}

// TestLive_AbsentExternalID verifies that GET for an unknown external id returns
// node=nil without surfacing an HTTP error.
func TestLive_AbsentExternalID(t *testing.T) {
	client := liveClient(t)
	uid := uniqueID()
	extID := "str:live-go-absent-" + uid

	resp, err := client.GetNodeByExternalID(context.Background(), extID)
	if err != nil {
		t.Fatalf("expected nil error for absent external id, got: %v", err)
	}
	if resp.Node != nil {
		t.Fatalf("expected nil node for absent external id, got node with id=%v", resp.Node.ID)
	}
}
