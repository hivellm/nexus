# Proposal: phase8_encryption-at-rest

## Why

Encryption at rest (database files encrypted with a key managed by the operator or a KMS) is a hard compliance requirement for SOC2, FedRAMP, HIPAA, GDPR, and PCI-DSS. Neo4j Enterprise ships it. Aura ships it by default. ArangoDB Enterprise ships it. Memgraph Enterprise ships it. Nexus's current posture is "rely on OS / disk-level encryption" — which is acceptable for a personal eval but disqualifying for any regulated customer. Implementing it inside the engine (transparent encryption of LMDB catalog + record stores + WAL + index files) keeps key management in the operator's hands and makes the whole package compliance-ready.

## What Changes

- Add a transparent encryption layer for the four on-disk surfaces: LMDB catalog, record stores (nodes / rels / props / strings), WAL, index files (B-tree, full-text, KNN, R-tree).
- Symmetric encryption: AES-256-GCM with per-page nonce derivation from `(file_id, page_offset)`. Keys never leave the process.
- Key management: master key supplied via env var `NEXUS_DATA_KEY` (raw or KMS reference); per-database derived keys via HKDF.
- KMS integration: optional adapter trait `KeyProvider` with implementations for AWS KMS, GCP KMS, HashiCorp Vault, file-based (default).
- Key rotation: online rotation by re-encrypting in the background (rotate-while-live).
- Activation: opt-in via `--encrypt-at-rest` flag; existing un-encrypted dbs migrate via a one-shot CLI command (`nexus admin encrypt-database <name>`).
- Document the threat model: encryption protects against disk theft + cold-snapshot exfiltration, not against runtime memory dumps.

## Impact

- Affected specs: new `docs/security/ENCRYPTION_AT_REST.md`, update `docs/security/AUTHENTICATION.md`.
- Affected code: new `crates/nexus-core/src/storage/crypto/`, hooks in catalog + record store + WAL + index modules, new CLI subcommand.
- Breaking change: opt-in (no impact unless flag set).
- User benefit: SOC2 / FedRAMP / HIPAA gate cleared; competitive parity with Neo4j Enterprise + Aura + Arango Ent + Memgraph Ent.
