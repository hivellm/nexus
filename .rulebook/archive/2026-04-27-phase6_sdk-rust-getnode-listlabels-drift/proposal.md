# Proposal: phase6_sdk-rust-getnode-listlabels-drift

Source: https://github.com/hivellm/nexus/issues/2

## Why

Smoke-testing the published Rust SDK (`nexus-graph-sdk = "1.14.0"`) against
`hivehub/nexus:v1.14.0` (Docker Hub) surfaced two API drifts and one minor
version-reporting bug that block the documented "create then read back"
flow shown in `examples/basic_usage.rs`. Cypher works end-to-end, but
SDK callers that follow the README pattern hit:

1. `client.get_node(id).node` returns `None` for nodes that were just
   created via `client.create_node(...)`. The same id is reachable via
   `MATCH (n) WHERE id(n) = $id RETURN n`, so the row exists — only the
   SDK's HTTP path silently maps to `None` (route missing or response
   shape mismatch deserialised into `Option::None`).
2. `client.list_labels().labels` and `client.list_rel_types().types`
   are typed `Vec<(String, u32)>`, but `examples/basic_usage.rs` and
   the README narrate `Vec<String>` and call `.iter().any(|l| l == &name)`,
   which fails to compile against the tuple type and is undocumented as
   to whether the second member is a count or something else.
3. `/health` reports `version=1.13.0` while the image tag is
   `hivehub/nexus:v1.14.0`. The binary's self-reported version lags the
   release tag because the workspace `Cargo.toml` was bumped after the
   last image build.

The author of issue #2 is integrating Nexus into `hivellm/cortex` and is
hitting (1) on the natural write-then-read pattern in their graph-writer
worker. They offered a PR but want the project to choose the shape for
(2) first. Severity is **low for write paths** (Cypher is unaffected),
**blocking for first-time SDK users**.

## What Changes

### 1. `get_node(id)` — fix the round-trip

- Audit the server route the SDK calls (currently `GET /nodes/{id}` or
  the equivalent under the JSON API surface) and confirm whether it is
  implemented and returning the expected shape.
- If the route is missing: implement it, returning the same JSON shape
  the SDK already deserialises (`Option<Node>` with `id`, `labels`,
  `properties`).
- If the route exists but the response shape drifted: align the
  response schema with the SDK's `GetNodeResponse` deserialisation
  target, OR update the SDK to match the server, OR rev both behind a
  versioned route. Pick **one** and document it.
- Add a regression integration test in `crates/nexus-server/tests/` that
  exercises `POST /nodes` → `GET /nodes/{id}` and asserts the returned
  body is `Some(...)` with the same labels/properties.
- Add an SDK-side integration test in `sdks/rust/tests/` that runs
  `create_node(...)` followed by `get_node(...)` against a live server
  fixture and asserts `Some`.

### 2. `list_labels` / `list_rel_types` — pick a final shape

Choose one of the two options below and apply consistently:

- **Option A (recommended): typed structs.** Introduce
  `pub struct LabelInfo { pub name: String, pub count: u32 }` and
  `pub struct RelTypeInfo { pub name: String, pub count: u32 }`. Change
  return types to `Vec<LabelInfo>` / `Vec<RelTypeInfo>`. Update the
  README and `examples/basic_usage.rs` walkthrough.
- **Option B: revert to `Vec<String>`.** Drop the count from the
  endpoint and expose label/type counts via the existing `get_stats`
  surface (`stats.catalog.label_count`, etc.). Update README and example.

Either option must update:

- `sdks/rust/src/client.rs` (or wherever the response types live)
- `sdks/rust/examples/basic_usage.rs`
- `sdks/rust/README.md`
- `crates/nexus-server` JSON response shape if the wire format changes

### 3. `/health` version drift — bump self-report to `1.14.0`

- Audit how `/health` derives its `version` field. If it reads from
  `CARGO_PKG_VERSION` at compile time, the discrepancy means the binary
  inside `hivehub/nexus:v1.14.0` was built from a tree where
  `crates/nexus-server/Cargo.toml` still said `1.13.0`. Verify the
  workspace `Cargo.toml` is now `1.14.0` (it is — confirmed
  `version.workspace = true` and the workspace root is `1.14.0`) and
  that the server crate inherits.
- Re-build and re-publish `hivehub/nexus:v1.14.0` from the current
  `main` commit so the self-reported version matches the tag.
- Add an integration test that asserts `health.version ==
  env!("CARGO_PKG_VERSION")` so the next release can't drift again.

## Impact

- **Affected specs**: none. This is a bugfix + API-shape decision; no
  spec change is required if Option B is chosen, and only a small
  Rust-side type change if Option A is chosen.
- **Affected code**:
  - `sdks/rust/src/client.rs` — `get_node`, `list_labels`,
    `list_rel_types` return types and deserialisation
  - `sdks/rust/examples/basic_usage.rs` — narration + iteration shape
  - `sdks/rust/README.md` — example block
  - `crates/nexus-server/src/api/` — `GET /nodes/{id}` route (or its
    equivalent) and the corresponding handler
  - `crates/nexus-server/src/api/health.rs` (or wherever `/health`
    builds its response) — sanity-check the `CARGO_PKG_VERSION` path
- **Breaking change**: **YES** if Option A is taken (SDK callers that
  destructure `Vec<(String, u32)>` need to migrate to `LabelInfo`).
  Either way, the SDK should bump to `1.15.0` once landed because the
  return-type signature changes.
- **User benefit**: first-time users following the README no longer
  hit a silent `None` on `get_node`, and the labels API has a single
  documented shape instead of an undocumented tuple.

## Source

- GitHub issue: https://github.com/hivellm/nexus/issues/2
- Reporter offered a PR — coordinate with them once the shape decision
  for #2 is made.
