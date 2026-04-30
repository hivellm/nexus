//! Encryption-at-rest primitives.
//!
//! # Why
//!
//! Encryption at rest — every byte the engine writes to disk is
//! ciphertext, every byte it reads back is decrypted before reaching
//! the executor — is a hard compliance requirement for SOC2,
//! FedRAMP, HIPAA, GDPR, and PCI-DSS workloads. Neo4j Enterprise,
//! Aura, ArangoDB Enterprise, and Memgraph Enterprise all ship it.
//! Nexus's previous posture was "rely on disk-level encryption" —
//! acceptable for a personal eval, disqualifying for any regulated
//! customer. This module is the engine-side foundation that makes
//! the storage layer compliance-ready when the storage hooks
//! (tracked under `phase8_encryption-at-rest-storage-hooks`) wire it
//! up.
//!
//! # Threat model
//!
//! Encryption at rest protects against:
//!
//! * **Disk theft.** A drive removed from the server reveals nothing
//!   without the master key.
//! * **Cold-snapshot exfiltration.** A volume snapshot or filesystem
//!   backup leaked to a third party reveals nothing.
//! * **Physical-media decommissioning.** Drives can be returned /
//!   destroyed without a wipe procedure.
//!
//! It does **not** protect against:
//!
//! * Runtime memory dumps. The master key + per-database keys live
//!   in process memory while the server runs.
//! * Hostile root on the running host. Nothing engine-side can stop
//!   that.
//! * Side-channel timing attacks. Out of scope; AES-GCM is constant-
//!   time on every CPU we ship for.
//!
//! See `docs/security/ENCRYPTION_AT_REST.md` for the operator-facing
//! threat model and KMS integration recipes.
//!
//! # Cryptographic choices
//!
//! * **AEAD**: AES-256-GCM (NIST SP 800-38D). 256-bit keys, 96-bit
//!   nonces, 128-bit auth tags. Available on every CPU Nexus
//!   targets via AES-NI / ARMv8 crypto extensions; AES-GCM is the
//!   default for at-rest encryption in every comparable engine.
//! * **KDF**: HKDF-SHA-256 (RFC 5869). Derives a per-database key
//!   from the master key + the database name (used as `info`
//!   string). This means rotating a database's key without
//!   touching the others is a one-shot HKDF call away.
//! * **Nonce derivation**: deterministic from `(file_id, page_offset,
//!   page_generation)`. AES-GCM is **catastrophically broken** under
//!   nonce reuse with the same key; we therefore commit to never
//!   reusing a `(key, nonce)` pair across writes via the page
//!   generation counter. The counter is stored in the page header and
//!   bumped on every overwrite.
//! * **Key zeroisation**: every secret is wrapped in
//!   [`zeroize::Zeroizing`] so it gets wiped from memory on drop.
//!
//! # Phase delivery
//!
//! This module ships the cryptographic core and the
//! [`KeyProvider`] abstraction. Wiring into the LMDB catalog,
//! record stores, WAL, and index files is tracked under separate
//! follow-up tasks (the storage modules each have their own
//! invariants and need a per-module review):
//!
//! * `phase8_encryption-at-rest-storage-hooks` — record stores +
//!   page cache integration.
//! * `phase8_encryption-at-rest-wal` — WAL append + replay path.
//! * `phase8_encryption-at-rest-indexes` — B-tree, full-text,
//!   KNN, R-tree.
//! * `phase8_encryption-at-rest-kms` — AWS KMS, GCP KMS, Vault
//!   adapters.
//! * `phase8_encryption-at-rest-rotation` — online key rotation
//!   with the two-key window.
//! * `phase8_encryption-at-rest-cli` — `nexus admin
//!   encrypt-database` / `rotate-key` subcommands.
//!
//! The follow-up tasks consume the API in this module without
//! changing it; the contracts are stable.

pub mod aes_gcm;
pub mod encrypted_file;
pub mod inventory;
pub mod kdf;
pub mod key_provider;
#[cfg(any(feature = "kms-aws", feature = "kms-gcp", feature = "kms-vault"))]
pub mod kms;
pub mod rotation;

pub use aes_gcm::{
    AeadError, NONCE_LEN, PageCipher, PageNonce, TAG_LEN, decrypt_page, encrypt_page,
};
pub use encrypted_file::{
    EncryptedPageStream, FileId, KeySource, PageBuffer, PageHeader, PageStreamError,
};
pub use inventory::{
    FileEncryptionState, InventoryError, InventoryReport, classify_file, enforce_uniform_state,
    scan_directory, scan_paths,
};
pub use kdf::{DatabaseKey, KdfError, MasterKey, derive_database_key};
pub use key_provider::{
    EnvKeyProvider, FileKeyProvider, KeyProvider, KeyProviderError, MASTER_KEY_LEN,
};
pub use rotation::{
    InMemoryPageStore, PageRef, PageStore, RotationCheckpoint, RotationError, RotationRunner,
    RotationRunnerConfig, RotationStats,
};
