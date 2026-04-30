# Proposal: phase8_encryption-at-rest-wal

## Why

The WAL is the durability path for every write the engine accepts. A leaked WAL segment is just as sensitive as a leaked record store. The cryptographic core in `phase8_encryption-at-rest` ships `EncryptedPageStream`; this task wires it into the WAL append + replay path so WAL segments are ciphertext on disk.

## What Changes

- Encrypt every WAL frame on append. The frame's existing CRC32C lives over the plaintext (so the integrity guarantee is end-to-end); the AEAD tag covers the ciphertext.
- Decrypt on replay. The replay path must tolerate a partial trailing frame (crash mid-write); ciphertext that fails AEAD validation is treated identically to a CRC mismatch (truncate + log).
- Verify the WAL replay test still passes under encryption.

## Impact

- Affected specs: `docs/specs/wal-mvcc.md`.
- Affected code: WAL module under `crates/nexus-core/src/`.
- Breaking change: NO (gated).
- User benefit: closes the WAL leak path for SOC2 / FedRAMP.
