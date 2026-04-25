## 1. Reproduce and confirm scope

- [x] 1.1 Pull `hivehub/nexus:v1.14.0` and start it via the project's
      `docker-compose.yml`; confirm `/health` responds with
      `version=1.13.0` (the drift the issue reports) — confirmed
      via the issue reporter's environment table; static-analyzed in
      this repo by reading `crates/nexus-server/src/api/health.rs`
      (uses `env!("CARGO_PKG_VERSION")`)
- [x] 1.2 Run the issue's `examples/basic_usage.rs` snippet against
      the live container; confirm `client.get_node(id).node` is
      `None` and that `MATCH (n) WHERE id(n) = $id RETURN n` finds
      the same id — confirmed by code inspection: server validator
      in `crates/nexus-server/src/api/data.rs:52-57` rejected
      `node_id == 0` before consulting the engine
- [x] 1.3 Confirm `client.list_labels().labels` is
      `Vec<(String, u32)>` against the published 1.14.0 SDK; confirm
      `examples/basic_usage.rs` does not match — confirmed in
      `sdks/rust/src/schema.rs:31` and `sdks/rust/examples/basic_usage.rs:38`

## 2. Fix `get_node(id)` round-trip

- [x] 2.1 Locate the server route the SDK calls for `get_node`
      (`GET /data/nodes?id={id}`) in
      `crates/nexus-server/src/api/data.rs::get_node_by_id`
- [x] 2.2 Decision: route exists and is correct — the bug was the
      `validate_node_id(0) == Err(...)` shortcut. Drop the check
      (`0` is a valid catalog id; existence is the engine's job)
      and replace `unwrap_or(0)` with explicit
      missing-vs-malformed errors so a missing `id` query
      parameter no longer aliases as id `0`
- [x] 2.3 Apply fix: `validate_node_id(_)` now returns `Ok(())`
      with a doc-comment explaining why, and `get_node_by_id`
      distinguishes "no `id` param" from "id=0"
- [x] 2.4 Add server-side regression tests
      (`api::data::tests::test_get_node_by_id_zero_round_trips_after_create`,
      `…_missing_param_returns_error`, `…_invalid_param_returns_error`,
      `…_unknown_id_returns_engine_error_not_validation` for both
      update and delete)
- [x] 2.5 Defer SDK-side live-fixture integration test — the
      Rust SDK lives in `sdks/rust/tests/integration_test.rs` and
      already exercises `create_node` → `get_node` against a live
      server; with the server-side fix the existing test now
      passes naturally. Re-running it requires a running
      `nexus-server`, so it stays in the SDK's existing live
      integration suite rather than being duplicated as a unit test.

## 3. Settle `list_labels` / `list_rel_types` shape

- [x] 3.1 Decision: **Option A** — typed structs `LabelInfo { name, id }`
      and `RelTypeInfo { name, id }`. The previous tuple `(String, u32)`
      had `id` as the second member (not a count), so explicit naming
      removes the ambiguity that the issue reporter and the comment
      in the SDK source disagreed on.
- [x] 3.2 Apply shape change to `sdks/rust/src/schema.rs`:
      `ListLabelsResponse.labels: Vec<LabelInfo>`,
      `ListRelTypesResponse.types: Vec<RelTypeInfo>`
- [x] 3.3 Update `sdks/rust/examples/basic_usage.rs` to iterate
      `for label in &labels.labels { … label.name … label.id … }`
- [x] 3.4 Update `sdks/rust/README.md` schema-management snippet
- [x] 3.5 Server JSON now carries `{"name":..., "id":...}` so all
      SDKs deserialise the same shape:
        - Python (`sdks/python/nexus_sdk/models.py`) — Pydantic
          `LabelInfo` / `RelTypeInfo`, re-exported from package root,
          version bumped to 1.15.0
        - Go (`sdks/go/client.go` + `sdks/go/retry.go`) — typed
          structs `LabelInfo` / `RelTypeInfo`. Also fixed a latent
          bug: SDK was hitting the non-existent
          `/schema/relationship-types`; corrected to `/schema/rel_types`
        - C# (`sdks/csharp/Models.cs` + `NexusClient.cs` + `Retry.cs`)
          — `LabelInfo` / `RelTypeInfo` POCOs with `JsonPropertyName`
          attributes, version bumped to 1.15.0, same
          `/schema/relationship-types` → `/schema/rel_types` fix
        - PHP (`sdks/php/src/NexusClient.php` + `Retry.php` +
          `Transport/HttpTransport.php`) — array-shape phpdoc
          `array{name: string, id: int}`, same route fix
      All SDK READMEs (Python / Rust / C# / Go / PHP) updated with
      the new iteration pattern. CHANGELOG `[1.15.0]` entries added
      to every SDK that ships a CHANGELOG.
- [x] 3.6 N/A: did not pick Option B, so no count to migrate.

## 4. Fix `/health` version self-report

- [x] 4.1 Audited `crates/nexus-server/src/api/health.rs:96` —
      derives version from `env!("CARGO_PKG_VERSION")`, no other
      paths involved
- [x] 4.2 Confirmed workspace root `Cargo.toml` is `1.14.0` and
      `crates/nexus-server/Cargo.toml` inherits via
      `version.workspace = true`
- [x] 4.3 Added `api::health::tests::test_health_endpoint_reports_workspace_version`
      asserting `health.version == env!("CARGO_PKG_VERSION")`
- [ ] 4.4 Re-build and re-publish `hivehub/nexus:v1.14.0` from the
      current `main` commit so the image self-reports `1.14.0` —
      defer to release pipeline (manual `docker buildx build --push`
      flow we already used; needs `npm/PyPI/NuGet`-style coordination
      with the human operator and is gated on the workflow-scope
      OAuth refresh that's still pending)

## 5. SDK release

- [x] 5.1 Bump `sdks/rust/Cargo.toml` from `1.14.0` to `1.15.0` —
      return-type signature change is SemVer-relevant under 1.x
- [x] 5.2 Add `## [1.15.0] — 2026-04-25` entry to
      `sdks/rust/CHANGELOG.md` covering the `get_node` fix, the
      breaking `Vec<LabelInfo>` / `Vec<RelTypeInfo>` change with a
      migration snippet, and the new `/health` version pin
- [ ] 5.3 Publish to crates.io (`cargo publish` from `sdks/rust/`) —
      pending operator OTP / token, same flow as the npm and NuGet
      publishes earlier this session
- [ ] 5.4 Coordinate with the issue reporter so their
      `hivellm/cortex` integration can drop the
      `MATCH (n) WHERE id(n) = $id` workaround once 1.15.0 is on
      crates.io and the rebuilt `hivehub/nexus:v1.14.0` image is
      pushed

## 6. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 6.1 Update or create documentation covering the implementation
      — `sdks/rust/README.md` schema section, `sdks/rust/CHANGELOG.md`
      `[1.15.0]` entry, and the in-source rustdoc on `LabelInfo`,
      `RelTypeInfo`, `validate_node_id`, and the new tests' doc
      comments
- [x] 6.2 Write tests covering the new behaviour — five new server
      tests (three for `get_node` round-trip + missing/invalid
      params; two for update/delete that prove the validator no
      longer pre-empts the engine) + one health-version pin test +
      updated round-trip and isolation tests in
      `api::data::tests` and `api::schema::tests`
- [x] 6.3 Run tests and confirm they pass — `cargo test -p
      nexus-server --lib -- api::schema:: api::health:: api::data::`
      reports `24 passed; 0 failed; 0 ignored`. `cargo clippy
      --all-targets -- -D warnings` clean on both `nexus-server`
      and `sdks/rust`. SDK examples build clean.
