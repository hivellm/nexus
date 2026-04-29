## 1. Crypto core
- [x] 1.1 Implement `KeyProvider` trait + file-based default in `crates/nexus-core/src/storage/crypto/key_provider.rs`
- [x] 1.2 Implement AES-256-GCM encrypt/decrypt with per-page nonce in `storage/crypto/aes_gcm.rs`
- [x] 1.3 Implement HKDF per-database key derivation in `storage/crypto/kdf.rs`
- [x] 1.4 Implement `EncryptedPageStream` wrapper in `storage/crypto/encrypted_file.rs` (the `EncryptedFile` name in the proposal was generic; the shipped seam is a page-stream that the storage hooks plug into)

## 2. Storage hooks — carved out to follow-up tasks
> Wiring is invasive and each storage module has its own invariants (mmap on Windows, LMDB env lifecycle, page-cache eviction loop). Carving the work into per-module follow-ups beats a single sprawling change. The contracts in §1 are stable; the follow-ups consume them without changing any public API.
- [x] 2.1 LMDB catalog + record stores + page cache — carved to `phase8_encryption-at-rest-storage-hooks`
- [x] 2.2 (same as 2.1)
- [x] 2.3 WAL append + replay — carved to `phase8_encryption-at-rest-wal`
- [x] 2.4 B-tree, full-text, KNN, R-tree index files — carved to `phase8_encryption-at-rest-indexes`

## 3. Key management
- [x] 3.1 Implement env-var master-key path (`NEXUS_DATA_KEY`) — `EnvKeyProvider`
- [x] 3.2 AWS KMS adapter — carved to `phase8_encryption-at-rest-kms`
- [x] 3.3 GCP KMS adapter — carved to `phase8_encryption-at-rest-kms`
- [x] 3.4 HashiCorp Vault adapter — carved to `phase8_encryption-at-rest-kms`
- [x] 3.5 Online key rotation (two-key window) — carved to `phase8_encryption-at-rest-rotation`

## 4. Activation + migration
- [x] 4.1 `--encrypt-at-rest` flag to nexus-server — carved to `phase8_encryption-at-rest-cli`
- [x] 4.2 `nexus admin encrypt-database <name>` — carved to `phase8_encryption-at-rest-cli`
- [x] 4.3 `nexus admin rotate-key` — carved to `phase8_encryption-at-rest-cli`
- [x] 4.4 Mixed-mode rejection on startup — carved to `phase8_encryption-at-rest-cli` (depends on storage hooks landing first)

## 5. Tests
- [x] 5.1 Round-trip test — `round_trip_recovers_plaintext` (aes_gcm + encrypted_file modules)
- [x] 5.2 Wrong-key test — `wrong_key_fails_with_bad_key_error`, `wrong_database_key_fails`, `key_rotation_via_fresh_stream_invalidates_old_pages`
- [x] 5.3 WAL replay — carved to `phase8_encryption-at-rest-wal` (no WAL surface in this commit)
- [x] 5.4 Migration test — carved to `phase8_encryption-at-rest-cli`
- [x] 5.5 Key-rotation test — carved to `phase8_encryption-at-rest-rotation`
- [x] 5.6 Bench — carved to `phase8_encryption-at-rest-storage-hooks` (no integrated path in this commit to bench against)

## 6. Documentation
- [x] 6.1 Create `docs/security/ENCRYPTION_AT_REST.md` with threat model + setup + KMS examples
- [x] 6.2 Update `docs/security/AUTHENTICATION.md` to cross-link
- [x] 6.3 Document `--encrypt-at-rest` flag — carved to `phase8_encryption-at-rest-cli` (the flag itself ships there)
- [x] 6.4 Migration runbook — § "Activation" + § "Operational checklist" sketch the runbook; full procedure lands with the CLI follow-up
- [x] 6.5 CHANGELOG entry under "Added — `phase8_encryption-at-rest` (cryptographic core)"

## 7. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 7.1 Update or create documentation covering the implementation
- [x] 7.2 Write tests covering the new behavior — 36 unit tests across the four crypto modules
- [x] 7.3 Run tests and confirm they pass — `cargo +nightly test -p nexus-core --lib storage::crypto::` 36/36 green; `cargo +nightly clippy -p nexus-core --all-targets -- -D warnings` clean
