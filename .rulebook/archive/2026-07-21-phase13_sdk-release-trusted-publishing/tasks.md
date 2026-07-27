# Tasks: phase13_sdk-release-trusted-publishing

Tag-driven SDK release CI with Trusted Publishing (OIDC, zero stored tokens) for
Rust, Python, TypeScript, and C# — mirror of Thunder's `release.yml` /
`release-train.yml`. PHP and Go SDKs are OUT of scope (moving to their own repos).

## 1. Manifest audit and alignment
- [x] 1.1 Audit the 4 manifests for publish-readiness and record the registry names/versions in the workflow comments: `sdks/rust/Cargo.toml` (crate name available on crates.io, no path/git deps, license/description/repository fields), `sdks/typescript/package.json` (`@hivehub` scope, `files`/`exports`, `publishConfig.access public`), `sdks/python/pyproject.toml` (PyPI name, build-system, classifiers), C# `.csproj` (`<Version>`, `<PackageId>`, license/repo metadata)
- [x] 1.2 Align the 4 manifest versions to one SDK version (they release as a train; tag `sdk-vX.Y.Z` must match all four) and fix any metadata gaps found in 1.1

## 2. Release workflow
- [x] 2.1 Create `.github/workflows/sdk-release.yml`: trigger `push: tags: ["sdk-v*"]` + `workflow_dispatch`; top-level `permissions: contents: read`; `gate` job = fmt/lint/test for the 4 SDKs (rust: fmt --check + clippy -D warnings + test; ts: npm ci + typecheck + lint + test; py: ruff + pytest; cs: `dotnet build -c Release -warnaserror` + `dotnet test --no-build`) + tag-vs-manifest check (strip `sdk-v` from `GITHUB_REF_NAME`, compare all 4, fail train on mismatch — port Thunder's `check` shell helper)
- [x] 2.2 `crates` job (`needs: gate`, `environment: crates`, `permissions: {contents: read, id-token: write}`): `dtolnay/rust-toolchain@stable` → `rust-lang/crates-io-auth-action@v1` → `cargo publish` with `CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}` from `sdks/rust`
- [x] 2.3 `npm` job (`environment: npm`, id-token): `actions/setup-node@v4` node 22 + `registry-url`, `npm install -g npm@latest` + version guard (trusted publishing needs npm ≥ 11.5.1), `npm ci && npm run build`, bare `npm publish --access public` — no NODE_AUTH_TOKEN, no `--provenance` flag (automatic)
- [x] 2.4 `pypi` job (`environment: pypi`, id-token): `actions/setup-python@v5`, `python -m build` + `twine check dist/*` in `sdks/python`, publish via `pypa/gh-action-pypi-publish@release/v1` with `packages-dir: sdks/python/dist`
- [x] 2.5 `nuget` job (`environment: nuget`, id-token): `actions/setup-dotnet@v4`, `dotnet pack -c Release`, `NuGet/login@v1` with `user: ${{ secrets.NUGET_USER }}` (profile name, not a credential), `dotnet nuget push` with exact-filename existence guard (`test -f`, version interpolated from tag — no glob) + `--skip-duplicate`

## 3. Verify and drift
- [x] 3.1 Port Thunder's `scripts/check_published_versions.py` to `scripts/ci/check_published_sdk_versions.py` for the 4 Nexus SDK packages (crates.io / npm / PyPI / NuGet APIs; `tag <version>` mode and `drift` mode; handle yanked versions and unreachable-vs-lagging registries; strip the `sdk-v` prefix)
- [x] 3.2 Add `verify` job to `sdk-release.yml` (`needs` all 4 publishers, `if: always()`): sleep ~120s for registry settle, then run the script in `tag` mode against `GITHUB_REF_NAME`
- [x] 3.3 Create `.github/workflows/sdk-release-train.yml`: weekly cron + `workflow_dispatch`, runs the script in `drift` mode (registries must agree with each other; repo ahead = passing)

## 4. Registry-side trusted publisher setup (documented runbook — owner executes)
- [x] 4.1 Write `docs/releases/SDK_RELEASE.md` runbook: create the 4 GitHub environments (`crates`, `npm`, `pypi`, `nuget`); register trusted publishers on crates.io / npmjs / pypi.org / nuget.org trustedpublishing with owner `hivellm`, repo `nexus`, workflow file `sdk-release.yml`, matching environment; set the non-secret `NUGET_USER` repo secret/variable (nuget.org profile name); release procedure = bump 4 manifests → `git tag sdk-vX.Y.Z` → push tag; note that registration must precede the first tagged run and recommend a `workflow_dispatch` dry run

## 5. Tail (docs + tests — check or waive with tailWaiver)
- [x] 5.1 Update or create documentation covering the implementation (`docs/releases/SDK_RELEASE.md` runbook from 4.1; release section in `sdks/README.md`; CHANGELOG entry)
- [x] 5.2 Write tests covering the new behavior (unit tests for `check_published_sdk_versions.py` — mocked registry responses for tag-match, drift-agree, yanked, unreachable cases; `actionlint` / YAML validation on both workflows)
- [x] 5.3 Run tests and confirm they pass (script tests green; `actionlint` clean; `workflow_dispatch` dry run of `sdk-release.yml` reaches the gate green — publish lanes expectedly no-op/fail-auth until registry registration is done, documented as such)
