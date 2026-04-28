## 1. Crypto core
- [ ] 1.1 Implement `KeyProvider` trait + file-based default in `crates/nexus-core/src/storage/crypto/key_provider.rs`
- [ ] 1.2 Implement AES-256-GCM encrypt/decrypt with per-page nonce in `storage/crypto/aes_gcm.rs`
- [ ] 1.3 Implement HKDF per-database key derivation
- [ ] 1.4 Implement `EncryptedFile` wrapper for memmap2-backed stores

## 2. Storage hooks
- [ ] 2.1 Wire encryption into LMDB catalog read/write (page-level)
- [ ] 2.2 Wire encryption into record-store read/write (page-level)
- [ ] 2.3 Wire encryption into WAL append + replay
- [ ] 2.4 Wire encryption into B-tree, full-text, KNN, R-tree index files

## 3. Key management
- [ ] 3.1 Implement env-var master-key path (`NEXUS_DATA_KEY`)
- [ ] 3.2 Implement AWS KMS adapter
- [ ] 3.3 Implement GCP KMS adapter
- [ ] 3.4 Implement HashiCorp Vault adapter
- [ ] 3.5 Implement online key rotation (re-encrypt in background, two-key window)

## 4. Activation + migration
- [ ] 4.1 Add `--encrypt-at-rest` flag to nexus-server
- [ ] 4.2 Add `nexus admin encrypt-database <name>` CLI subcommand for one-shot migration
- [ ] 4.3 Add `nexus admin rotate-key` CLI subcommand
- [ ] 4.4 Reject mixed-mode (some files encrypted, some not) on startup with clear error

## 5. Tests
- [ ] 5.1 Round-trip test: write encrypted, read back identical
- [ ] 5.2 Wrong-key test: decryption fails cleanly with `ERR_BAD_KEY`
- [ ] 5.3 WAL replay test: encrypted WAL replays correctly after crash
- [ ] 5.4 Migration test: un-encrypted db → encrypted db → verify data
- [ ] 5.5 Key-rotation test: rotate while serving traffic, verify continuity
- [ ] 5.6 Bench: measure throughput overhead vs un-encrypted (target ≤ 15 %)

## 6. Documentation
- [ ] 6.1 Create `docs/security/ENCRYPTION_AT_REST.md` with threat model + setup + KMS examples
- [ ] 6.2 Update `docs/security/AUTHENTICATION.md` to cross-link
- [ ] 6.3 Document `--encrypt-at-rest` flag in nexus-server CLI ref
- [ ] 6.4 Add migration runbook
- [ ] 6.5 CHANGELOG entry

## 7. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 7.1 Update or create documentation covering the implementation
- [ ] 7.2 Write tests covering the new behavior
- [ ] 7.3 Run tests and confirm they pass
