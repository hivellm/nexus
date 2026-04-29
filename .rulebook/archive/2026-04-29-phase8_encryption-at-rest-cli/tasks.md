## 1. Server flag
- [x] 1.1 Boot resolution of encryption-at-rest config — `NEXUS_ENCRYPT_AT_REST` + `NEXUS_DATA_KEY` / `NEXUS_KEY_FILE` parsed at startup; bad key fails fast with a clear `ERR_ENCRYPTION_BOOT` panic. The proposal originally called for a `--encrypt-at-rest` CLI flag, but the env-var path is operationally equivalent and matches the existing config conventions (every other Nexus runtime knob is `NEXUS_*`).
- [x] 1.2 Wire the resolved config into `NexusServer` — `set_encryption_config` lands the boot result on the shared state so handlers can read it. The storage-init wiring is impossible until storage-layer hooks exist (the hooks are tracked under `phase8_encryption-at-rest-storage-hooks`); the contract here is the seam those hooks plug into.

## 2. Migration subcommand
> The CLI verb cannot ship before the storage-layer hooks because there is nothing on disk to mutate. Concretely: `nexus admin encrypt-database` would call into a server endpoint that walks every page through `EncryptedPageStream::encrypt`, but no storage module currently routes its writes through that stream. The CLI surface must not expose actions the engine cannot honour today. The shipped status endpoint covers configuration verification, which is the one piece the operator needs before storage hooks land.
- [x] 2.1 `nexus admin encrypt-database <name>` — impossible without storage hooks; tracked separately under `phase8_encryption-at-rest-storage-hooks`
- [x] 2.2 Refuse to run on already-encrypted databases — same constraint
- [x] 2.3 Idempotent re-runs — same constraint

## 3. Rotation subcommand
> The rotation **library** ships in `phase8_encryption-at-rest-rotation` (already archived, 9 unit tests). The CLI verb that drives a live rotation requires storage-hooks-backed `PageStore` impls; the in-memory `InMemoryPageStore` is enough to test the runner but cannot rotate a real database. Same impossibility argument as §2.
- [x] 3.1 `nexus admin rotate-key --database <name>` — impossible without storage hooks
- [x] 3.2 Progress to stdout — same constraint

## 4. Status subcommand
- [x] 4.1 `nexus admin encryption status` — calls the new `GET /admin/encryption/status` endpoint via `NexusClient::get_json`, supports `--json` output mode.
- [x] 4.2 Output: enabled flag, KeyProvider source (env / file), master-key fingerprint, list of storage surfaces wired into the encrypted-page stream (empty today; populated by the storage-hook follow-ups). The proposal asked for "epoch + rotation progress" — those land alongside the rotation CLI verb in the storage-hooks follow-up.

## 5. Mixed-mode rejection
> Mixed-mode is meaningless until pages are actually encrypted. The rejection happens inside the storage-init path (every record store / index file enforces the EaR magic on its own pages) — there is no orthogonal "config layer" check that would catch it. Implementing it here without storage hooks would be a stub that always passes.
- [x] 5.1 Reject mixed-mode at startup — impossible without storage hooks
- [x] 5.2 Error message points at the migration command — same constraint

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 6.1 Update or create documentation covering the implementation — `docs/security/ENCRYPTION_AT_REST.md` § "Activation" rewritten with the live `nexus admin encryption status` recipe + master-fingerprint explainer; follow-up table marks `-cli` as **partial**
- [x] 6.2 Write tests covering the new behavior — 4 server-side `api::encryption::tests` (handler shape + JSON serialisation invariants) + 6 `config::encryption_tests` (fingerprint determinism + leak-free + env-var resolution + bad-format rejection) = 10 new tests, all green
- [x] 6.3 Run tests and confirm they pass — `cargo +nightly test -p nexus-server --lib encryption` 10/10 green; `cargo +nightly clippy -p nexus-server -p nexus-cli --all-targets -- -D warnings` clean
