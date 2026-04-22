package transport

import (
	"context"
	"encoding/binary"
	"fmt"
	"io"
	"net"
	"sync"
	"sync/atomic"
	"time"
)

// pushID is reserved by the server for PUSH frames — clients must skip it.
const pushID uint32 = 0xFFFFFFFF

// RpcTransport is a single-socket native binary RPC client.
//
// One TCP stream is opened lazily on the first request. The writer is
// guarded by a mutex so frames never interleave; a single reader
// goroutine multiplexes responses back to pending callers keyed by
// request ID.
type RpcTransport struct {
	endpoint       Endpoint
	creds          Credentials
	connectTimeout time.Duration

	connMu  sync.Mutex
	conn    net.Conn
	writeMu sync.Mutex

	pendingMu sync.Mutex
	pending   map[uint32]chan RpcResponse

	nextID atomic.Uint32
	closed atomic.Bool

	readerDone chan struct{}
}

// NewRpcTransport builds a fresh RPC transport. The connection is
// opened lazily on the first call to [RpcTransport.Execute] / [Call].
func NewRpcTransport(endpoint Endpoint, creds Credentials) *RpcTransport {
	t := &RpcTransport{
		endpoint:       endpoint,
		creds:          creds,
		connectTimeout: 5 * time.Second,
		pending:        make(map[uint32]chan RpcResponse),
	}
	t.nextID.Store(1)
	return t
}

// SetConnectTimeout tunes the TCP-level connect timeout.
func (t *RpcTransport) SetConnectTimeout(d time.Duration) { t.connectTimeout = d }

// Execute implements [Transport].
func (t *RpcTransport) Execute(ctx context.Context, req Request) (Response, error) {
	resp, err := t.Call(ctx, req.Command, req.Args)
	if err != nil {
		return Response{}, err
	}
	val, err := resp.Unwrap()
	if err != nil {
		return Response{}, err
	}
	return Response{Value: val}, nil
}

// Describe implements [Transport].
func (t *RpcTransport) Describe() string {
	return fmt.Sprintf("%s (RPC)", t.endpoint)
}

// IsRpc implements [Transport].
func (t *RpcTransport) IsRpc() bool { return true }

// Close implements [Transport].
func (t *RpcTransport) Close() error {
	if !t.closed.CompareAndSwap(false, true) {
		return nil
	}
	t.connMu.Lock()
	conn := t.conn
	t.conn = nil
	t.connMu.Unlock()
	if conn != nil {
		_ = conn.Close()
	}
	if t.readerDone != nil {
		<-t.readerDone
	}
	t.failAll(fmt.Errorf("RPC transport closed"))
	return nil
}

// Call sends a single request without the [Request] wrapper.
func (t *RpcTransport) Call(ctx context.Context, command string, args []NexusValue) (RpcResponse, error) {
	if err := t.ensureConnected(ctx); err != nil {
		return RpcResponse{}, err
	}
	return t.send(ctx, RpcRequest{ID: t.allocID(), Command: command, Args: args})
}

// ── Internals ──────────────────────────────────────────────────────────

func (t *RpcTransport) allocID() uint32 {
	for {
		id := t.nextID.Add(1) - 1
		// Wrap before overflow.
		if id >= 0xFFFFFFFE {
			t.nextID.Store(1)
			continue
		}
		if id == pushID {
			continue
		}
		return id
	}
}

func (t *RpcTransport) ensureConnected(ctx context.Context) error {
	t.connMu.Lock()
	defer t.connMu.Unlock()
	if t.conn != nil || t.closed.Load() {
		if t.closed.Load() {
			return fmt.Errorf("RPC transport closed")
		}
		return nil
	}

	dialer := net.Dialer{Timeout: t.connectTimeout}
	authority := t.endpoint.Authority()
	conn, err := dialer.DialContext(ctx, "tcp", authority)
	if err != nil {
		return fmt.Errorf("failed to connect to %s: %w", authority, err)
	}
	if tcpConn, ok := conn.(*net.TCPConn); ok {
		_ = tcpConn.SetNoDelay(true)
	}
	t.conn = conn
	t.readerDone = make(chan struct{})
	go t.readLoop(conn, t.readerDone)

	// HELLO + AUTH handshake.
	if err := t.handshakeLocked(ctx); err != nil {
		_ = conn.Close()
		t.conn = nil
		return err
	}
	return nil
}

func (t *RpcTransport) handshakeLocked(ctx context.Context) error {
	// We're holding connMu but not using it for send — sendUnlocked
	// uses writeMu so this is safe.
	hello, err := t.sendUnlocked(ctx, RpcRequest{ID: 0, Command: "HELLO", Args: []NexusValue{NxInt(1)}})
	if err != nil {
		return fmt.Errorf("failed to send HELLO: %w", err)
	}
	if !hello.OK {
		return fmt.Errorf("HELLO rejected by server: %s", hello.Err)
	}
	if !t.creds.HasAny() {
		return nil
	}
	var authArgs []NexusValue
	if t.creds.APIKey != "" {
		authArgs = []NexusValue{NxStr(t.creds.APIKey)}
	} else {
		authArgs = []NexusValue{NxStr(t.creds.Username), NxStr(t.creds.Password)}
	}
	auth, err := t.sendUnlocked(ctx, RpcRequest{ID: 0, Command: "AUTH", Args: authArgs})
	if err != nil {
		return fmt.Errorf("failed to send AUTH: %w", err)
	}
	if !auth.OK {
		return fmt.Errorf("authentication failed: %s", auth.Err)
	}
	return nil
}

// send acquires the pending-map slot and waits for the reader goroutine
// to deliver the matching response.
func (t *RpcTransport) send(ctx context.Context, req RpcRequest) (RpcResponse, error) {
	return t.sendUnlocked(ctx, req)
}

func (t *RpcTransport) sendUnlocked(ctx context.Context, req RpcRequest) (RpcResponse, error) {
	if t.closed.Load() {
		return RpcResponse{}, fmt.Errorf("RPC transport closed")
	}
	t.connMu.Lock()
	conn := t.conn
	t.connMu.Unlock()
	if conn == nil {
		return RpcResponse{}, fmt.Errorf("RPC transport is not connected")
	}

	ch := make(chan RpcResponse, 1)
	t.pendingMu.Lock()
	t.pending[req.ID] = ch
	t.pendingMu.Unlock()

	frame, err := EncodeRequestFrame(req)
	if err != nil {
		t.popPending(req.ID)
		return RpcResponse{}, err
	}

	t.writeMu.Lock()
	if deadline, ok := ctx.Deadline(); ok {
		_ = conn.SetWriteDeadline(deadline)
	}
	_, werr := conn.Write(frame)
	_ = conn.SetWriteDeadline(time.Time{})
	t.writeMu.Unlock()
	if werr != nil {
		t.popPending(req.ID)
		return RpcResponse{}, fmt.Errorf("failed to send RPC frame: %w", werr)
	}

	select {
	case resp, ok := <-ch:
		if !ok {
			return RpcResponse{}, fmt.Errorf("RPC connection closed")
		}
		return resp, nil
	case <-ctx.Done():
		t.popPending(req.ID)
		return RpcResponse{}, ctx.Err()
	}
}

func (t *RpcTransport) popPending(id uint32) {
	t.pendingMu.Lock()
	delete(t.pending, id)
	t.pendingMu.Unlock()
}

func (t *RpcTransport) readLoop(conn net.Conn, done chan<- struct{}) {
	defer close(done)
	header := make([]byte, 4)
	for {
		if _, err := io.ReadFull(conn, header); err != nil {
			t.failAll(fmt.Errorf("RPC connection closed: %w", err))
			return
		}
		length := binary.LittleEndian.Uint32(header)
		body := make([]byte, length)
		if _, err := io.ReadFull(conn, body); err != nil {
			t.failAll(fmt.Errorf("RPC read failed: %w", err))
			return
		}
		resp, err := DecodeResponseBody(body)
		if err != nil {
			t.failAll(fmt.Errorf("malformed RPC frame: %w", err))
			return
		}
		t.pendingMu.Lock()
		ch, ok := t.pending[resp.ID]
		if ok {
			delete(t.pending, resp.ID)
		}
		t.pendingMu.Unlock()
		if ok {
			ch <- resp
		}
		// Unknown IDs (including PUSH_ID) are dropped — push subscriptions
		// are not wired on the SDK side yet.
	}
}

func (t *RpcTransport) failAll(err error) {
	t.pendingMu.Lock()
	for id, ch := range t.pending {
		// Close the channel so blocked senders unblock with "closed" semantics.
		_ = id
		close(ch)
	}
	t.pending = make(map[uint32]chan RpcResponse)
	t.pendingMu.Unlock()
	_ = err
}
