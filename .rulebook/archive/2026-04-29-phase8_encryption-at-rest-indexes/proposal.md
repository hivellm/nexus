# Proposal: phase8_encryption-at-rest-indexes

## Why

Indexes hold derivable but still sensitive data — a B-tree index on `email` reveals every email address in the database; a full-text index reveals tokenised document content. SOC2 / FedRAMP requires every on-disk surface be encrypted, not just the record stores.

## What Changes

- Wire `EncryptedPageStream` into the B-tree index file format (`FileId::BTreeIndex`).
- Wire into the Tantivy full-text index. Tantivy's segment layout uses multiple files per segment; each gets a unique on-disk file id.
- Wire into the HNSW KNN index — sequential append-mostly file layout, simpler than the B-tree case.
- Wire into the R-tree spatial index.

## Impact

- Affected specs: `docs/specs/page-cache.md`, `docs/specs/knn-integration.md`, `docs/specs/rtree-index.md`.
- Affected code: `crates/nexus-core/src/index/`.
- Breaking change: NO (gated).
- User benefit: full coverage of the on-disk threat model.
