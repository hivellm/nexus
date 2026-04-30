//! `/admin/encryption/*` — operator surface for encryption-at-rest.
//!
//! # Scope
//!
//! Phase 8 ships the cryptographic core
//! (`crates/nexus-core/src/storage/crypto/`) and the rotation runner
//! (`phase8_encryption-at-rest-rotation`). Storage-layer wiring
//! (LMDB catalog, record stores, WAL, indexes) is tracked under
//! separate follow-up tasks; until those land, the only thing the
//! server can honestly report is the **boot-time configuration** —
//! which provider sourced the master key, and the SHA-256
//! fingerprint of that key.
//!
//! This module exposes that read-only configuration via
//! `GET /admin/encryption/status`. The fingerprint is safe to log;
//! the key itself is never serialised.
//!
//! Migration / rotation / KMS subcommands and their backing
//! endpoints land alongside the storage-hooks follow-up so the wire
//! surface only grows once the engine actually does something with
//! encryption.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::NexusServer;
use crate::config::{EncryptionConfig, EncryptionInventorySummary, EncryptionSource};

/// JSON shape returned by `GET /admin/encryption/status`. Stable
/// across releases; new fields are additive.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EncryptionStatusReport {
    /// `true` when `NEXUS_ENCRYPT_AT_REST=true` was set at boot
    /// AND the configured [`KeyProvider`] resolved a valid key.
    /// `false` for the default plaintext deployment.
    ///
    /// [`KeyProvider`]: nexus_core::storage::crypto::KeyProvider
    pub enabled: bool,
    /// Where the master key came from. `None` when `enabled = false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<EncryptionSource>,
    /// SHA-256 fingerprint of the master key (`nexus:<16-hex>` —
    /// safe to log; safe to publish). Operators verify two replicas
    /// share the same master by comparing fingerprints. `None` when
    /// `enabled = false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// On-disk inventory recovered at boot — counts of plaintext /
    /// encrypted / empty files in the data directory. `None` when
    /// the boot scan was skipped or the data dir was missing. The
    /// counts let operators verify the data dir is in the expected
    /// state without exposing per-file paths to a remote caller.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inventory: Option<EncryptionInventorySummary>,
    /// Phase-8 storage-layer wiring is staged across multiple
    /// follow-up tasks. This array enumerates the on-disk surfaces
    /// that have been wired into the encrypted-page stream so far —
    /// today, the empty list. The follow-ups
    /// (`phase8_encryption-at-rest-storage-hooks`,
    /// `-wal`, `-indexes`) will append to it without bumping the
    /// envelope version.
    pub storage_surfaces: Vec<&'static str>,
    /// API-version field. Bumped on every breaking shape change.
    /// Today: `1`.
    pub schema_version: u32,
}

impl From<&EncryptionConfig> for EncryptionStatusReport {
    fn from(cfg: &EncryptionConfig) -> Self {
        Self {
            enabled: cfg.enabled,
            source: cfg.source.clone(),
            fingerprint: cfg.fingerprint.clone(),
            inventory: cfg.inventory.clone(),
            storage_surfaces: Vec::new(),
            schema_version: 1,
        }
    }
}

/// `GET /admin/encryption/status` handler.
pub async fn status(State(server): State<Arc<NexusServer>>) -> Json<EncryptionStatusReport> {
    Json(EncryptionStatusReport::from(&server.encryption_config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_from_disabled_config() {
        let cfg = EncryptionConfig::default();
        let r = EncryptionStatusReport::from(&cfg);
        assert!(!r.enabled);
        assert!(r.source.is_none());
        assert!(r.fingerprint.is_none());
        assert!(r.storage_surfaces.is_empty());
        assert_eq!(r.schema_version, 1);
    }

    #[test]
    fn report_from_env_sourced_config() {
        let cfg = EncryptionConfig {
            enabled: true,
            source: Some(EncryptionSource::Env {
                name: "NEXUS_DATA_KEY".into(),
            }),
            fingerprint: Some("nexus:0123456789abcdef".into()),
            inventory: None,
        };
        let r = EncryptionStatusReport::from(&cfg);
        assert!(r.enabled);
        assert!(matches!(
            r.source,
            Some(EncryptionSource::Env { ref name }) if name == "NEXUS_DATA_KEY"
        ));
        assert_eq!(r.fingerprint.as_deref(), Some("nexus:0123456789abcdef"));
    }

    #[test]
    fn report_serialises_to_documented_json_shape() {
        let cfg = EncryptionConfig {
            enabled: true,
            source: Some(EncryptionSource::File {
                path: "/etc/nexus/master.key".into(),
            }),
            fingerprint: Some("nexus:abcd1234".into()),
            inventory: Some(EncryptionInventorySummary {
                empty: 1,
                plaintext: 0,
                encrypted: 4,
            }),
        };
        let r = EncryptionStatusReport::from(&cfg);
        let json = serde_json::to_value(&r).expect("serialise");
        assert_eq!(json["enabled"], true);
        assert_eq!(json["source"]["kind"], "file");
        assert_eq!(json["source"]["path"], "/etc/nexus/master.key");
        assert_eq!(json["fingerprint"], "nexus:abcd1234");
        assert_eq!(json["schema_version"], 1);
        // `storage_surfaces` is always present (additive contract);
        // omitting it would let consumers special-case the empty
        // case in subtle ways.
        assert!(json.get("storage_surfaces").is_some());
        // `inventory` is present iff the boot scan ran. Counts
        // surface as flat fields; no per-file path leakage.
        assert_eq!(json["inventory"]["empty"], 1);
        assert_eq!(json["inventory"]["plaintext"], 0);
        assert_eq!(json["inventory"]["encrypted"], 4);
    }

    #[test]
    fn report_omits_optional_fields_when_disabled() {
        let cfg = EncryptionConfig::default();
        let r = EncryptionStatusReport::from(&cfg);
        let json = serde_json::to_value(&r).expect("serialise");
        // `source` and `fingerprint` carry `skip_serializing_if`.
        assert!(json.get("source").is_none());
        assert!(json.get("fingerprint").is_none());
        assert_eq!(json["enabled"], false);
    }
}
