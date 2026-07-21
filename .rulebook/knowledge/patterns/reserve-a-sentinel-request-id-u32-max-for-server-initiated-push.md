# Reserve a sentinel request id (u32::MAX) for server-initiated push

**Category**: architecture
**Tags**: rpc, wire-format, forward-compat, streaming

## Description

Request/response protocols that may one day grow server-initiated push frames (streaming results, live-query subscriptions, pub/sub) need a forward-compatible way to distinguish push from reply. The pattern: reserve a single sentinel id (we picked `u32::MAX`, named `PUSH_ID`), teach every SDK to skip it in the allocator, and make the server reject client-originated requests that use it. Push frames can then land at any time with `id == PUSH_ID` and the client demux routes them to a subscription channel instead of the pending-id map.

## Example

// Server-side (nexus-protocol/src/rpc/mod.rs):
pub const PUSH_ID: u32 = u32::MAX;
// Client-side: every SDK's allocator skips it.
// Rust:
fn alloc_id(&self) -> u32 {
    let id = self.next_id.fetch_add(1, Ordering::Relaxed);
    if id == PUSH_ID { self.next_id.fetch_add(1, Ordering::Relaxed) } else { id }
}
// Server: phase1_nexus-rpc-binary-protocol dispatcher rejects
// client-originated Request.id == PUSH_ID with ERR.

## When to Use

Any RPC protocol where a wire-level schema change to add push semantics later would be painful. Costs one id value out of 4 billion, bought forever.

## When NOT to Use

Short-lived protocols where every connection is a single request/response (no persistent channel to push over). Protocols that already have an explicit stream frame type (HTTP/2, gRPC) — their framing layer handles direction without needing a sentinel id.
