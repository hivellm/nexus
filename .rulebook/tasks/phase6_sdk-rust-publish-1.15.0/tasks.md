## 1. Re-build and re-publish `hivehub/nexus:v1.14.0`

- [ ] 1.1 Verify workspace root `Cargo.toml` at release commit reports `1.14.0` (or current release tag) and that `crates/nexus-server/Cargo.toml` inherits via `version.workspace = true`
- [ ] 1.2 `docker buildx build --push --tag hivehub/nexus:v1.14.0 .` from a clean `main` checkout (operator: needs Docker Hub creds)
- [ ] 1.3 Smoke-test: `docker run --rm -p 15474:15474 hivehub/nexus:v1.14.0`, `curl http://localhost:15474/health`, assert `version` matches the binary's `CARGO_PKG_VERSION`

## 2. Publish `nexus-graph-sdk@1.15.0` to crates.io

- [ ] 2.1 `cd sdks/rust && cargo publish --dry-run` and confirm clean
- [ ] 2.2 `cargo publish` from `sdks/rust/` (operator: needs crates.io API token)
- [ ] 2.3 Tag release commit `sdk-rust-v1.15.0` and push the tag

## 3. Close out issue #2

- [ ] 3.1 Comment on https://github.com/hivellm/nexus/issues/2 summarising the three fixes (`get_node` round-trip, `LabelInfo`/`RelTypeInfo` typed structs, `/health` version pin), the crates.io version, and the rebuilt image tag
- [ ] 3.2 Ask the reporter to verify against `hivellm/cortex`; close the issue once confirmed

## 4. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 4.1 Update or create documentation covering the implementation — release notes link in `sdks/rust/CHANGELOG.md` `[1.15.0]` entry pointing to the crates.io release URL, and a one-line note in `docs/` if a release-process doc exists
- [ ] 4.2 Write tests covering the new behaviour — N/A: this task only ships pre-built artifacts. The behavioural tests landed in the parent task (`api::data::tests`, `api::schema::tests`, `api::health::tests::test_health_endpoint_reports_workspace_version`)
- [ ] 4.3 Run tests and confirm they pass — re-run the parent task's gate (`cargo test -p nexus-server --lib -- api::schema:: api::health:: api::data::`) against the release commit before publishing
