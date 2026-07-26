# Proposal: phase12_thunder-interop-validation

## Why

Phases 10–11 change the transport under every client in six languages. Synap's 1.2.0
release gate for the identical migration was a **cross-SDK interop matrix**
(`scripts/interop/run-matrix.py`): one server with auth required on off-default ports,
one thin client per language driving the transport layer directly, four wire-focused
steps, plus a `legacy` cell replaying the pre-Thunder wire to prove backward
compatibility. That gate caught every real bug in Synap's migration (missing AUTH on
reconnect, PUSH_ID collisions, non-UTF-8 corruption, allocation-before-validation) —
per-SDK unit suites did not. Nexus needs the same gate before the migration can be
called done, plus proof that the untouched surfaces (HTTP Neo4j compat suite, RESP3,
transport parity) did not regress.

## What Changes

- New `scripts/interop/` harness (adapted from Synap's): `run-matrix.py` boots one
  `nexus-server` from a checked-in config (off-default ports, `NEXUS_RPC_REQUIRE_AUTH`
  on), runs each language client, renders a pass/fail matrix. Client contract identical
  to Synap's: `argv: <host> <port> <user-or-key> <pass>`, stdout `STEP <name> PASS|FAIL
  <detail>`, exit 0 on all-pass.
- Per-language interop clients in `scripts/interop/clients/{rust,python,typescript,go,
  csharp,php}/` driving the SDK transport layer directly (the matrix is about the wire).
  Steps adapted to Nexus: `auth` (gated command before/after AUTH — probe with STATS,
  not PING, since PING answers pre-auth), `cypher` (CREATE + MATCH round-trip),
  `knn_bytes` (embedding as raw f32-LE `Bytes` == `Array<Float>`, byte-exact result),
  `error` (server error surfaces typed and the connection stays usable).
- A `legacy` cell replaying the pre-Thunder Nexus wire (map-shaped Request, Bytes as
  int-array) against the new server — proves deployed old SDKs keep working.
- Results recorded in `docs/protocol/thunder-interop-matrix.md` (Synap convention).
- Regression proof for untouched surfaces: the Neo4j 300-test HTTP suite, RESP3 smoke,
  and `test-transport-parity.sh` re-run and recorded.

### Risks / notes

- The matrix needs all 6 toolchains on the host; mirror Synap's per-cell toolchain
  override (`SYNAP_INTEROP_<CELL>` → `NEXUS_INTEROP_<CELL>`) so missing toolchains
  skip explicitly rather than fail silently.
- Off-default ports (Synap used 25500/25501/26379) so the matrix never collides with a
  dev server.

## Impact

- Affected specs: `docs/specs/rpc-wire-format.md` (final state), `docs/specs/sdk-transport.md`
- Affected code: new `scripts/interop/**`, `docs/protocol/thunder-interop-matrix.md`;
  no production code changes expected (findings feed back into phase10/11 fixes)
- Breaking change: NO
- User benefit: released migration is provably wire-compatible across all 6 languages
  and backward-compatible with deployed clients; the matrix becomes a permanent release
  gate for future transport work (as it is in Synap).

## References

- Synap gate: `e:\HiveLLM\Synap\scripts\interop\{run-matrix.py,server-config.yml,clients/}`,
  `docs/protocol/thunder-interop-matrix.md`, CHANGELOG 1.2.0 interop section.
- Thunder conformance corpus (reusable vectors): `e:\HiveLLM\Thunder\conformance\vectors\`,
  `interop/run.py`.
- Nexus surfaces to keep green: `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`
  (HTTP), `scripts/compatibility/test-transport-parity.sh` (HTTP/RPC/RESP3).
