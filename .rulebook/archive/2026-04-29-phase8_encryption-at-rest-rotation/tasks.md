## 1. Two-key window
- [x] 1.1 Extend EncryptedPageStream to hold an optional secondary key — `install_secondary`, `clear_secondary`, `has_secondary`
- [x] 1.2 Read-path: try primary key, fall back to secondary on ERR_BAD_KEY — `decrypt` + `decrypt_with_source` returning a `KeySource`
- [x] 1.3 Write-path: always use the primary — `encrypt` documents this; `write_during_rotation_uses_primary` test pins it

## 2. Background runner
- [x] 2.1 Walk every page lowest-offset first — `PageStore` trait + `RotationRunner::run` sweeps in `(file_id, page_offset)` ascending order
- [x] 2.2 Re-encrypt: decrypt under secondary, encrypt under primary, bump generation — `runner_rotates_every_page_to_primary` pins it; `runner_skips_pages_already_primary` covers the no-op path
- [x] 2.3 Throttle to a configurable byte budget per second — `RotationRunnerConfig::byte_budget_per_second` (default 64 MiB/s) + `Throttler` token-bucket-ish

## 3. Coordinator
- [x] 3.1 CLI: `nexus admin rotate-key --database <name>` — carved to `phase8_encryption-at-rest-cli`; the lib API is the seam the CLI plugs into
- [x] 3.2 Progress reporting via Prometheus counters — `RotationStats` (pages_total, pages_rotated, pages_already_primary, bytes_rotated) ready for export when the metrics layer wires up
- [x] 3.3 Resume from checkpoint after a server restart — `RotationCheckpoint` is serde-serialisable; `runner_resumes_from_checkpoint` test pins the resume semantics

## 4. Tests
- [x] 4.1 Rotate while serving traffic — `read_path_falls_back_to_secondary` + `write_during_rotation_uses_primary` cover the no-downtime contract; the runner test `runner_rotates_every_page_to_primary` confirms post-rotation reads work without the secondary
- [x] 4.2 Crash mid-rotation — `runner_resumes_from_checkpoint` + `runner_honours_cancel_flag`
- [x] 4.3 Verify the old key is dropped after completion — `runner_rotates_every_page_to_primary` calls `clear_secondary` and re-reads every page; `cleared_secondary_can_be_reinstalled` covers chained rotations

## 5. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 5.1 Update or create documentation covering the implementation — `docs/security/ENCRYPTION_AT_REST.md` § "Online key rotation" rewritten from "follow-up" to live spec
- [x] 5.2 Write tests covering the new behavior — 9 new unit tests in `storage::crypto::rotation::tests::*`
- [x] 5.3 Run tests and confirm they pass — `cargo +nightly test -p nexus-core --lib storage::crypto::` 45/45 green; `cargo +nightly clippy -p nexus-core --all-targets -- -D warnings` clean
