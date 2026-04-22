# Operating the Nexus RPC transport

> Audience: platform / infra engineers promoting Nexus 1.0.0 to
> production. Developers just using the SDK should read
> `docs/MIGRATION_SDK_TRANSPORT.md` instead.

This runbook covers everything you need to roll out, monitor, and
troubleshoot the native binary RPC listener alongside the existing
HTTP endpoint. Keep the wire-format spec ([`docs/specs/rpc-wire-format.md`](specs/rpc-wire-format.md))
open in a second window — this document references it constantly.

## 1. Ports and firewall posture

| Port   | Protocol    | Use                                     | Default bind |
|--------|-------------|-----------------------------------------|--------------|
| 15474  | HTTP/JSON   | REST API (SDK fallback, browser, tools) | `127.0.0.1`  |
| 15475  | Binary RPC  | **Default SDK transport** (MessagePack) | `127.0.0.1`  |
| 15476  | RESP3       | `redis-cli` diagnostic tail             | Disabled     |

Rules of thumb:

- **Open `15475` wherever your SDKs connect** — that's where the RPC
  default lands. Most applications need only this port.
- **Keep `15474` open too** if you run a browser dashboard, use
  `curl`-based ops scripts, or front the database with an HTTP-aware
  load balancer (nginx, HAProxy, cloud ALB). LBs cannot rewrite the
  binary RPC stream.
- **Keep `15476` loopback-only** unless you've got operators using
  `redis-cli` / `iredis` to poke the database. It's a diagnostic
  port, not a public surface.

Example `iptables` rule for a Nexus server fronted by an external app
tier:

```sh
# RPC + HTTP open to the app tier subnet; RESP3 loopback only.
iptables -A INPUT -p tcp -s 10.0.1.0/24 --dport 15474 -j ACCEPT
iptables -A INPUT -p tcp -s 10.0.1.0/24 --dport 15475 -j ACCEPT
iptables -A INPUT -p tcp       -s 127.0.0.1 --dport 15476 -j ACCEPT
iptables -A INPUT -p tcp --dport 15475 -j DROP
```

## 2. Bind addresses

`NEXUS_ADDR` / `NEXUS_RPC_ADDR` set the listen address.

- **Single-tenant**: `127.0.0.1:15475` is fine. Keep RPC loopback and
  front with a socket proxy if you must cross hosts.
- **Multi-tenant / cloud**: bind the RPC listener to the private
  subnet NIC (`10.x.x.x:15475`). Avoid `0.0.0.0` unless the VPC's
  security-group rules already gate ingress.
- **Shared infrastructure**: pair RPC with an internal load balancer
  that does **TCP passthrough** (not HTTP L7). Lock the allow-list to
  the application tier.

## 3. TLS posture (1.0.0)

Native TLS for RPC is **not shipped in 1.0.0**. Three supported
patterns:

1. **Terminate TLS at an internal load balancer** (AWS NLB with ACM,
   GCP Internal TCP LB with SSL, Azure Internal LB). The LB forwards
   plain RPC to the Nexus server over a private subnet.
2. **Terminate TLS in a sidecar** (`stunnel`, Envoy TCP proxy) running
   on the same host. Nexus listens on `127.0.0.1:15475`; the sidecar
   handles TLS and forwards to loopback.
3. **Use HTTPS** (`https://host:443`) if TLS termination infrastructure
   is unavailable. HTTP/JSON is slower than RPC but ships with TLS
   support via `reqwest` / `HttpClient` / `axios`.

TLS for RPC is tracked for V2 alongside the clustering TLS work.

## 4. Prometheus metrics

Scrape the existing `/prometheus` HTTP endpoint — it already carries
RPC counters:

| Metric                                          | Type    | Description                                        |
|-------------------------------------------------|---------|----------------------------------------------------|
| `nexus_rpc_connections`                         | gauge   | Live RPC TCP connections                           |
| `nexus_rpc_commands_total`                      | counter | Total RPC commands dispatched                      |
| `nexus_rpc_commands_error_total`                | counter | Commands that returned an error                    |
| `nexus_rpc_command_duration_microseconds_total` | counter | Sum of handler wall-clock μs                       |
| `nexus_rpc_frame_bytes_in_total`                | counter | Bytes of incoming frame payloads                   |
| `nexus_rpc_frame_bytes_out_total`               | counter | Bytes of outgoing frame payloads                   |
| `nexus_rpc_slow_commands_total`                 | counter | Commands exceeding `rpc.slow_threshold_ms`         |
| `nexus_audit_log_failures_total`                | counter | Audit-log append failures (fail-open policy)       |

**Derived views worth building in Grafana:**

- `rate(nexus_rpc_commands_total[1m])` — RPC QPS.
- `rate(nexus_rpc_command_duration_microseconds_total[1m]) /
   rate(nexus_rpc_commands_total[1m])` — average μs / command.
- `rate(nexus_rpc_slow_commands_total[5m]) /
   rate(nexus_rpc_commands_total[5m])` — slow-command ratio.
- `rate(nexus_rpc_frame_bytes_in_total[1m]) +
   rate(nexus_rpc_frame_bytes_out_total[1m])` — total RPC bandwidth.

**Suggested alert thresholds:**

- Slow-command ratio > **5%** for 10 minutes → paging.
- Error ratio > **1%** over 5 minutes → paging.
- `nexus_rpc_connections` persistently above `max_in_flight_per_conn`
  × 10 → warning (possible client leak).

## 5. Rate limits + DOS posture

Defaults tuned for single-digit thousand-QPS workloads; raise as
needed:

```toml
[rpc]
max_frame_bytes = 67_108_864        # 64 MiB per body
max_in_flight_per_conn = 1024       # per-conn semaphore
slow_threshold_ms = 2               # logs WARN above this
```

Environment overrides (`NEXUS_RPC_MAX_FRAME_BYTES`,
`NEXUS_RPC_MAX_IN_FLIGHT`, `NEXUS_RPC_SLOW_MS`) take precedence.

For DOS resistance, pair the above with per-IP connection caps at the
network layer:

- **Nginx stream** (`nginx.conf`):
  ```nginx
  stream {
    limit_conn_zone $binary_remote_addr zone=rpc_per_ip:10m;
    server {
      listen 15475;
      limit_conn rpc_per_ip 64;
      proxy_pass nexus_backend;
    }
  }
  ```
- **iptables** (`--connlimit-above 64`).
- **Cloud LB** connection-limit rules per source IP.

## 6. Logs + tracing

Every RPC connection gets a `tracing` span `rpc.conn {peer, id}`;
every request gets `rpc.req {id, cmd}`. Commands exceeding
`rpc.slow_threshold_ms` log at WARN:

```
WARN rpc.req{id=42 cmd=CYPHER}: slow command: 15.7ms query="MATCH ..."
```

Correlate with `X-Request-Id` on the HTTP side through the shared
request-id middleware — any request that spans both transports
preserves the id.

## 7. Failure-mode playbook

| Symptom                                              | First check                                       | Fix                                                                                 |
|------------------------------------------------------|---------------------------------------------------|-------------------------------------------------------------------------------------|
| Clients report `SLOW_COMMAND` warnings               | `rate(nexus_rpc_slow_commands_total[5m])`         | Inspect planner cache hit rate; tune `planner.cache_size`; consider GDS indexes.    |
| `frame body X bytes exceeds limit Y bytes`           | Query logs for client IP                          | Tune `NEXUS_RPC_MAX_FRAME_BYTES` up or batch the payload on the client.             |
| `failed to connect` but HTTP works                   | `ss -tlnp` on the server; firewall rules          | Ensure RPC listener bound on the right interface; open port 15475 in the firewall.  |
| `HELLO rejected by server`                           | Server version                                    | Upgrade client; the server advertises `HELLO.proto=1` and rejects older protocols.  |
| `authentication failed`                              | API key / basic auth on both transports           | Verify the key hasn't expired; RPC sends AUTH right after HELLO on each connection. |
| `RPC id mismatch (expected N, got M)`                | Client concurrency model                          | Ensure the client serialises writes per connection (every SDK does this by default).|
| Unexpected `TIMEOUT` errors                          | `rpc.max_in_flight_per_conn` in config            | Raise the limit, or pool more connections.                                          |
| Memory usage grows linearly with connections         | `nexus_rpc_connections` vs. connection TTL        | Clients should reuse long-lived connections; check for socket leaks in callers.     |

## 8. Rollout checklist (v0.12 → 1.0.0)

1. **Deploy server 1.0.0** — the RPC listener is on by default on
   port `15475`. HTTP continues on `15474`.
2. **Open `15475`** in security groups / iptables for the app-tier
   subnet.
3. **Set `NEXUS_SDK_TRANSPORT=http` globally** if you want a
   defensive staged rollout — this keeps every SDK on HTTP until you
   flip it.
4. **Upgrade SDK callers to 1.0.0** (see `docs/MIGRATION_SDK_TRANSPORT.md`).
5. **Unset the env var** (or set it to `nexus`) to promote RPC to
   default.
6. **Watch** `nexus_rpc_commands_total` rise and
   `nexus_rpc_commands_error_total` stay flat for at least one peak
   hour.
7. **Rollback path**: set `NEXUS_SDK_TRANSPORT=http` on the callers;
   they'll fall back instantly. The server-side RPC listener can be
   disabled by setting `NEXUS_RPC_ENABLED=false` and restarting.

## 9. Cross-references

- Wire format: [`docs/specs/rpc-wire-format.md`](specs/rpc-wire-format.md).
- SDK contract: [`docs/specs/sdk-transport.md`](specs/sdk-transport.md).
- SDK migration: [`docs/MIGRATION_SDK_TRANSPORT.md`](MIGRATION_SDK_TRANSPORT.md).
- RESP3 vocabulary: [`docs/specs/resp3-nexus-commands.md`](specs/resp3-nexus-commands.md).
- Root architecture: [`docs/ARCHITECTURE.md`](ARCHITECTURE.md).
