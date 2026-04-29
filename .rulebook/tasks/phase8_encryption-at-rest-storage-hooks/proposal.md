# Proposal: phase8_encryption-at-rest-storage-hooks

## Why

The cryptographic core landed in `phase8_encryption-at-rest`
(`crates/nexus-core/src/storage/crypto/`). It exposes
`EncryptedPageStream` — the seam — but the LMDB catalog, record
stores (nodes / rels / props / strings), and the page cache still
write plaintext to disk. Wiring is gated separately because each
storage module has its own invariants (mmap-backed record stores
on Windows, LMDB's environment lifecycle, the page-cache eviction
loop) and a per-module review beats a single sprawling change.

## What Changes

- Wire `EncryptedPageStream` into the LMDB catalog read/write path.
- Wire the page stream into the node / relationship / property /
  string record stores. Each store is mmap-backed; encryption
  happens at page boundaries (`PAGE_SIZE = 8 KiB`) on the
  write-back path.
- Wire into the page cache (`crates/nexus-core/src/cache/`):
  ciphertext on disk, plaintext in cache lines.
- Add a startup invariant: a database with `encryption.enabled = true`
  must have **every** on-disk page tagged with the EaR magic
  (`0x4E58_4350`). Mixed mode is rejected with a clear error.
- Round-trip + crash-recovery + benchmark tests against a
  representative workload.

## Impact

- Affected specs: `docs/specs/storage-format.md`, `docs/specs/page-cache.md`.
- Affected code: `crates/nexus-core/src/catalog/`, `storage/`, `cache/`.
- Breaking change: NO (gated behind `--encrypt-at-rest`).
- User benefit: catalog + record-store + page-cache compliance with the SOC2 / FedRAMP threat model.
