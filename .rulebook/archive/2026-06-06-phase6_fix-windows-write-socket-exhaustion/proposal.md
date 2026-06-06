# Proposal: phase6_fix-windows-write-socket-exhaustion

Reconfirmed in production (Cortex bootstrap against Nexus over HTTP on Windows).

## Why
Issuing one write per HTTP request exhausts sockets on Windows: each
request opens a new TCP connection that lands in `TIME_WAIT`, and the
ephemeral port range drains under sustained write load, causing
connection failures. Callers currently mitigate with a batch of 40 +
retry + item-by-item fallback — a client-side band-aid that hides a
server/transport defect and caps write throughput. The server (and/or
the first-party SDK transport) should support connection reuse so a
high-volume write workload does not deplete OS sockets.

## What Changes
- Determine where the per-request connection churn originates: server-side
  HTTP keep-alive configuration (Axum/hyper) and/or the SDK/protocol
  client opening a fresh connection per request instead of reusing a
  pooled, keep-alive connection.
- Ensure HTTP keep-alive is enabled and honored end-to-end so repeated
  writes reuse a small pool of connections (no per-request socket
  churn / TIME_WAIT pileup).
- Provide a batched / pipelined write path (or confirm keep-alive alone
  resolves it) so the documented client workaround (batch 40 + retry +
  item-by-item fallback) is no longer required.
- Validate on Windows specifically (ephemeral port range / TIME_WAIT
  behaviour differs from Linux) under a sustained write load.

## Impact
- Affected specs: api-protocols (transport / connection management)
- Affected code: `crates/nexus-server/src/` (HTTP server keep-alive
  config), `crates/nexus-protocol/src/` (client transport / connection
  reuse), relevant SDK transports under `sdks/`
- Breaking change: NO
- User benefit: sustained high-volume writes on Windows without socket
  exhaustion; no client-side batching/retry workaround; higher write
  throughput
