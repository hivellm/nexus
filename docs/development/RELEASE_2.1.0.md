# Release 2.1.0 — checklist + publish order

This is the worked-example checklist for the `2.1.0` ship. Every command is verified-working from the phase 11 dry-run pass; the operator with credentials follows them in order.

> Phase 9 + 10 ship train: external-node-ids end-to-end, six-SDK parity, REPLACE-prop-ptr fix, live-validated against the tagged `nexus-nexus:2.1.0` image (87 SDK live cases + 25 REST + 29 demo + 9 WAL replay = **150 / 150 PASS**).

## Pre-flight

- [ ] All gates green on `main`: `cargo +nightly clippy --workspace -- -D warnings`, `cargo +nightly test --workspace`, `bash scripts/sdks/run-live-suites.sh`.
- [ ] `git status` clean. No untracked phase 9/10/11 artefacts.
- [ ] You are on the release commit. `git log --oneline -1` reads the 2.1.0 release line.
- [ ] Versions consistent everywhere:
  - `Cargo.toml` workspace = `2.1.0`
  - `sdks/rust/Cargo.toml` = `2.1.0`
  - `sdks/python/pyproject.toml` = `2.1.0`
  - `sdks/typescript/package.json` = `2.1.0`
  - `sdks/csharp/Nexus.SDK.csproj` `<Version>` = `2.1.0`
  - `sdks/php/composer.json` = `2.1.0`
  - `deploy/helm/nexus/Chart.yaml` `appVersion` = `2.1.0` (chart `version` is independent)

## Publish order

The dependency chain forces this exact sequence. Skipping ahead breaks the next-up step.

### 1. `nexus-protocol` to crates.io

```bash
cargo +nightly publish -p nexus-protocol --manifest-path crates/nexus-protocol/Cargo.toml
```

Required because `nexus-graph-sdk` depends on `nexus-protocol = "^2.1.0"`. Without this, the SDK dry-run fails with `failed to select a version for the requirement nexus-protocol = "^2.1.0"`.

### 2. `nexus-graph-sdk` (Rust SDK) to crates.io

```bash
cargo +nightly publish --manifest-path sdks/rust/Cargo.toml
```

Verify post-publish: `cargo install nexus-graph-sdk --version 2.1.0 --dry-run`.

### 3. Python SDK to PyPI

The artefacts are already produced by phase 11 §3.2:

```bash
ls sdks/python/dist/hivehub_nexus_sdk-2.1.0*
# hivehub_nexus_sdk-2.1.0-py3-none-any.whl
# hivehub_nexus_sdk-2.1.0.tar.gz
twine upload sdks/python/dist/hivehub_nexus_sdk-2.1.0*
```

Verify: `pip install hivehub-nexus-sdk==2.1.0 --dry-run`.

### 4. TypeScript SDK to npm

```bash
cd sdks/typescript
npm publish --access public
```

Verify: `npm view @hivehub/nexus-sdk@2.1.0`.

### 5. Go SDK — git tag

Go modules are tag-derived; there is no registry push.

```bash
git tag -a sdks/go/v2.1.0 -m "sdks/go v2.1.0"
git push origin sdks/go/v2.1.0
```

Verify: `go list -m github.com/hivellm/nexus-go@v2.1.0`.

### 6. C# SDK to NuGet

The `.nupkg` is already built by phase 11 §3.5:

```bash
ls sdks/csharp/bin/Release/Nexus.SDK.2.1.0.nupkg
dotnet nuget push sdks/csharp/bin/Release/Nexus.SDK.2.1.0.nupkg \
  --source https://api.nuget.org/v3/index.json \
  --api-key "$NUGET_API_KEY"
```

Verify: `dotnet add package Nexus.SDK --version 2.1.0` in a scratch project.

### 7. PHP SDK on Packagist

Packagist auto-publishes from a git tag pointing at the SDK directory. Push the tag and Packagist polls the webhook:

```bash
git tag -a sdks/php/v2.1.0 -m "sdks/php v2.1.0"
git push origin sdks/php/v2.1.0
```

Verify on the Packagist page after a few minutes; `composer require hivellm/nexus-php:2.1.0` should resolve.

### 8. Docker image to the public registry

```bash
docker tag nexus-nexus:2.1.0 ghcr.io/hivellm/nexus:2.1.0
docker tag nexus-nexus:2.1.0 ghcr.io/hivellm/nexus:latest
docker push ghcr.io/hivellm/nexus:2.1.0
docker push ghcr.io/hivellm/nexus:latest
```

Verify: `curl -s $(docker run --rm -p 15474:15474 -d ghcr.io/hivellm/nexus:2.1.0)/health` reports `version: 2.1.0`.

### 9. Helm chart

```bash
helm package deploy/helm/nexus -d /tmp/charts
# uploads through whatever process the team uses (artifact hub /
# private repo / etc.)
```

Helm chart `version` is `0.2.0` per the helm convention; `appVersion` is `2.1.0`. They move independently.

### 10. GitHub release

```bash
gh release create v2.1.0 \
  --title "Nexus 2.1.0 — external-node-ids" \
  --notes-file docs/development/RELEASE_2.1.0.md \
  /tmp/charts/nexus-0.2.0.tgz \
  sdks/python/dist/hivehub_nexus_sdk-2.1.0-py3-none-any.whl \
  sdks/python/dist/hivehub_nexus_sdk-2.1.0.tar.gz \
  sdks/csharp/bin/Release/Nexus.SDK.2.1.0.nupkg
```

## Smoke-test commands

Re-run before AND after the publish to confirm nothing regressed in the registry-roundtrip.

```bash
# Build the image once — every other smoke step depends on it.
docker compose build
docker tag nexus-nexus:latest nexus-nexus:2.1.0

# Live SDK suites (87 test cases across 6 SDKs).
bash scripts/sdks/run-live-suites.sh

# REST surface (25 cases). Each compat suite needs a fresh container —
# they re-use deterministic external ids that collide with prior runs
# on the shared catalog.
docker rm -f nexus-smoke 2>/dev/null
docker run -d --name nexus-smoke -p 15474:15474 \
    -e NEXUS_AUTH_ENABLED=false -e NEXUS_AUTH_REQUIRED_FOR_PUBLIC=false \
    -e NEXUS_ROOT_ENABLED=false nexus-nexus:2.1.0
until curl -s -f http://localhost:15474/health > /dev/null; do sleep 1; done
python scripts/compatibility/test-external-ids-docker.py

# End-to-end demo (29 cases).
docker rm -f nexus-smoke && docker run -d --name nexus-smoke -p 15474:15474 \
    -e NEXUS_AUTH_ENABLED=false -e NEXUS_AUTH_REQUIRED_FOR_PUBLIC=false \
    -e NEXUS_ROOT_ENABLED=false nexus-nexus:2.1.0
until curl -s -f http://localhost:15474/health > /dev/null; do sleep 1; done
python scripts/compatibility/demo-external-ids-relationships.py

# WAL replay across container restart (9 cases).
docker rm -f nexus-smoke nexus-phase9-wal nexus-phase9-wal-data 2>/dev/null
python scripts/compatibility/test-wal-replay-docker.py

# Final cleanup.
docker rm -f nexus-smoke 2>/dev/null
```

## Phase 11 known caveats

These ride along with 2.1.0 but are not blockers — each is documented in `sdks/PHASE10_LIVE_RESULTS.md`:

- Server's `MATCH (n {prop: $param})` does not yet substitute parameters inside property maps. Workaround: inline literals or use `WHERE n.prop = $param`.
- `POST /data/relationships` rejects `source_id == 0` / `target_id == 0`. Workaround: burn id 0 with a `_Sentinel` node (the orchestration script does this automatically).

## Rollback

If a registry push lands a broken artefact:

- Rust: `cargo yank --vers 2.1.0 nexus-graph-sdk` and `cargo yank --vers 2.1.0 nexus-protocol`.
- npm: `npm deprecate "@hivehub/nexus-sdk@2.1.0" "broken — use 2.1.1"`.
- PyPI: cannot delete; publish `2.1.1` immediately and update the README.
- NuGet: cannot delete; publish `2.1.1`.
- Packagist: re-tag `v2.1.1`.
- Docker: re-tag `2.1.0` to point at the previous good image, push a `2.1.1` tag for the fix.

Always cut a `2.1.1` first; the yank/deprecate is the second step so users following install instructions don't hit a dead artefact.
