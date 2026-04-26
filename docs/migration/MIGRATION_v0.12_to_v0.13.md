# Migrating from v0.12 to v1.0.0 (née v0.13)

> The "v0.13" that `phase3_rpc-protocol-docs-benchmarks` targeted
> shipped as **1.0.0** — we promoted the version during the same
> release cut so every server / SDK / CLI shares a single semver
> line. This guide keeps the v0.13 filename for historical
> continuity.

## What changed, in one paragraph

Nexus 1.0.0 adds a **native binary RPC transport** on port `15475`
alongside the existing HTTP endpoint on `15474` and the opt-in RESP3
debug port on `15476`. All six first-party SDKs (Rust, Python,
TypeScript, Go, C#, PHP) default to RPC. The HTTP endpoint is
unchanged, so servers keep serving existing clients until they
upgrade.

## Operator checklist

1. **Firewall**: open TCP `15475` wherever your SDKs connect. Keep
   `15474` open too if you have HTTP callers (browsers, ops scripts,
   HTTP-aware load balancers).
2. **Config**: no changes required. `NEXUS_RPC_ENABLED=true` is the
   default; set it to `false` to disable the new listener.
3. **Restart**: stop and start the server. The RPC listener starts
   alongside the existing HTTP listener; no downtime beyond the
   normal restart window.
4. **Verify**: `curl http://host:15474/health` (HTTP) and any
   RPC-capable SDK (`nexus://host:15475`) should both succeed.

## SDK-user checklist

**Most callers need to do nothing.** The SDK defaults flip from HTTP
to RPC; if port `15475` is reachable, the new default takes effect on
upgrade. If it isn't, the opt-out path is one env var.

| Scenario                                         | What to do                                                |
|--------------------------------------------------|-----------------------------------------------------------|
| You passed an explicit `http://…` URL to the SDK | **Nothing.** Your code keeps hitting port `15474`.        |
| You relied on the default base URL               | Make sure the server exposes `15475` — or set `NEXUS_SDK_TRANSPORT=http`. |
| You use CRUD helpers (`createNode`, etc.)        | **Nothing.** They keep using the sibling HTTP port.       |
| You catch typed errors (`*nexus.Error`, …)       | **Nothing.** Error types are preserved across transports. |

Detailed per-SDK opt-out recipes live in [`MIGRATION_SDK_TRANSPORT.md`](MIGRATION_SDK_TRANSPORT.md).

## Rollback procedure

If the RPC listener misbehaves and you need to fall back to HTTP-only
traffic:

1. Set `NEXUS_SDK_TRANSPORT=http` on every caller. SDKs switch
   immediately to the HTTP path.
2. Optionally disable the server-side RPC listener:
   `NEXUS_RPC_ENABLED=false`, then restart.
3. Verify `nexus_rpc_connections` drops to zero on the Prometheus
   scrape.
4. File an issue — the rollback itself should always be painless, so
   if it wasn't, we need to know.

## Cross-references

- Operator runbook: [`docs/OPERATING_RPC.md`](OPERATING_RPC.md)
- SDK migration: [`docs/MIGRATION_SDK_TRANSPORT.md`](MIGRATION_SDK_TRANSPORT.md)
- Wire format: [`docs/specs/rpc-wire-format.md`](specs/rpc-wire-format.md)
- SDK transport contract: [`docs/specs/sdk-transport.md`](specs/sdk-transport.md)
