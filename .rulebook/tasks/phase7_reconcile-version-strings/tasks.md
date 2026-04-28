## 1. Implementation
- [ ] 1.1 Audit current versions across README, CHANGELOG, all `Cargo.toml`, all SDK package manifests
- [ ] 1.2 Decide canonical policy (single train vs decoupled SDK train) and document in `docs/development/RELEASE_PROCESS.md`
- [ ] 1.3 Create `docs/COMPATIBILITY_MATRIX.md` with current server↔protocol↔SDK mapping
- [ ] 1.4 Update README badge + top CHANGELOG entry to converge on next release version
- [ ] 1.5 Add SDK README "compatible with server ≥ X.Y" line to each of 6 SDKs
- [ ] 1.6 Add `scripts/ci/check_version_consistency.sh` that fails on README↔Cargo.toml↔CHANGELOG mismatch
- [ ] 1.7 Wire the script into the CI workflow

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
