//! Encryption-at-rest configuration: types, master-key resolution
//! (env / file / KMS), and the boot-time data-directory invariant scan.
//!
//! Extracted from `config.rs` as a pure mechanical move — every type and
//! function below is byte-identical to the original; only this module
//! boundary and the re-exports in `config/mod.rs` are new.

/// Encryption-at-rest configuration. Resolved from
/// `NEXUS_ENCRYPT_AT_REST` + `NEXUS_DATA_KEY` / `NEXUS_KEY_FILE` at
/// server boot.
#[derive(Debug, Clone, Default)]
pub struct EncryptionConfig {
    /// Master switch. `false` keeps the storage layer in plaintext
    /// (the pre-phase-8 behaviour); `true` requires a valid
    /// [`KeyProvider`] source to be configured. Storage hooks gate
    /// on this flag once they land.
    ///
    /// [`KeyProvider`]: nexus_core::storage::crypto::KeyProvider
    pub enabled: bool,
    /// Where the master key is sourced from. `None` when
    /// `enabled = false` or when boot resolution failed and the
    /// server intentionally started without a key (we never silently
    /// fall through — a hard fail is the default).
    pub source: Option<EncryptionSource>,
    /// SHA-256 fingerprint of the resolved master key (32 bytes,
    /// hex-encoded). Safe to log; safe to ship over the
    /// `/admin/encryption/status` endpoint. Lets operators verify
    /// two servers are using the same key without exposing the key
    /// itself.
    pub fingerprint: Option<String>,
    /// On-disk inventory recovered at boot — counts of plaintext
    /// vs encrypted files in the data directory. `None` when the
    /// scan was skipped (no data dir resolved) or when encryption
    /// is disabled and no encrypted files were found (the trivial
    /// case has nothing to report). Set by
    /// [`enforce_data_dir_invariants`] from the boot path.
    pub inventory: Option<EncryptionInventorySummary>,
}

/// Operator-facing summary of the on-disk encryption inventory.
/// Counts only — paths are not surfaced over `/admin/encryption/
/// status` because they leak filesystem layout to a remote
/// caller; the boot log carries the full path list when a
/// mixed-mode error fires.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Default)]
pub struct EncryptionInventorySummary {
    /// Files visited that classified as zero-byte / shorter than
    /// the page header (no opinion).
    pub empty: usize,
    /// Files whose first page is plaintext (no EaR magic).
    pub plaintext: usize,
    /// Files whose first page parses as a valid encrypted page
    /// header.
    pub encrypted: usize,
}

/// Tag identifying which [`KeyProvider`] backed the resolved master
/// key. Wider than `KeyProviderError` because the operator might
/// want to know *which* env var or *which* path was used.
///
/// [`KeyProvider`]: nexus_core::storage::crypto::KeyProvider
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EncryptionSource {
    /// Master key sourced from an environment variable.
    Env { name: String },
    /// Master key sourced from a file on disk.
    File { path: String },
    /// Master key sourced from a managed KMS adapter (AWS / GCP /
    /// Vault). The `provider` tag is the operator-supplied name
    /// (`aws` / `gcp` / `vault`); `label` is the adapter's
    /// `KeyProvider::label()` — provider-specific identifier safe to
    /// log (KMS key ARN, GCP key resource path, Vault transit
    /// mount/key).
    Kms { provider: String, label: String },
}

/// Resolve the master key from the configured source and compute a
/// SHA-256 fingerprint. The fingerprint is safe to log — it's a
/// hash of the key, not the key itself; two servers booting with
/// the same master will report the same fingerprint, two servers
/// with different masters will report different ones.
///
/// Returns `Ok(None)` when `enabled = false` (no key resolution
/// attempted) and `Err` when resolution itself failed (bad path,
/// wrong format, missing env var). The server's boot path treats
/// the latter as fatal — silently falling through would let an
/// operator who *thought* they had encryption running ship to
/// production without it.
pub fn resolve_encryption_config() -> anyhow::Result<EncryptionConfig> {
    let enabled = std::env::var("NEXUS_ENCRYPT_AT_REST")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false);
    if !enabled {
        return Ok(EncryptionConfig::default());
    }

    use nexus_core::storage::crypto::{
        EnvKeyProvider, FileKeyProvider, KeyProvider, MASTER_KEY_LEN,
    };

    // Resolution order:
    //   1. NEXUS_KMS_PROVIDER ∈ {aws, gcp, vault} — managed KMS path
    //      (only available when nexus-core was built with the
    //      matching `kms-*` feature; an unbuilt provider surfaces a
    //      hard fail with a clear message).
    //   2. NEXUS_KEY_FILE — file-backed master key.
    //   3. NEXUS_DATA_KEY — env-backed master key.
    let (source, key) = if let Ok(provider) = std::env::var("NEXUS_KMS_PROVIDER") {
        resolve_kms(&provider)?
    } else if let Ok(path) = std::env::var("NEXUS_KEY_FILE") {
        let p = FileKeyProvider::from_path(&path)
            .map_err(|e| anyhow::anyhow!("failed to load NEXUS_KEY_FILE={path}: {e}"))?;
        let k = p
            .master_key()
            .map_err(|e| anyhow::anyhow!("failed to read master key from {path}: {e}"))?;
        (EncryptionSource::File { path: path.clone() }, *k)
    } else {
        let p = EnvKeyProvider::from_default_env()
            .map_err(|e| anyhow::anyhow!("failed to load NEXUS_DATA_KEY env var: {e}"))?;
        let k = p
            .master_key()
            .map_err(|e| anyhow::anyhow!("failed to read NEXUS_DATA_KEY: {e}"))?;
        (
            EncryptionSource::Env {
                name: "NEXUS_DATA_KEY".to_string(),
            },
            *k,
        )
    };
    debug_assert_eq!(key.len(), MASTER_KEY_LEN);

    Ok(EncryptionConfig {
        enabled: true,
        source: Some(source),
        fingerprint: Some(fingerprint_master_key(&key)),
        inventory: None,
    })
}

/// Boot-time invariant: scan `data_dir` for the EaR magic on every
/// regular file, reject mixed-mode databases (some files
/// encrypted, others plaintext), and reject configurations whose
/// on-disk state contradicts the encryption flag (encrypted files
/// under `enabled = false`, or plaintext files under
/// `enabled = true`).
///
/// On success, returns `cfg` augmented with an
/// [`EncryptionInventorySummary`] so the operator log + the
/// `/admin/encryption/status` endpoint can surface the recovered
/// counts. On failure, returns an `anyhow` error that carries the
/// exact `ERR_ENCRYPTION_*` taxonomy from the inventory module —
/// the boot path treats it as fatal.
///
/// `data_dir` may legitimately not exist yet on first boot; the
/// scanner returns an empty report and the invariant accepts it
/// under either flag value.
pub fn enforce_data_dir_invariants(
    cfg: EncryptionConfig,
    data_dir: &std::path::Path,
) -> anyhow::Result<EncryptionConfig> {
    use nexus_core::storage::crypto::{enforce_uniform_state, scan_directory};

    let report = scan_directory(data_dir).map_err(|e| {
        anyhow::anyhow!(
            "ERR_ENCRYPTION_BOOT: failed to scan {} for the EaR invariant: {e}",
            data_dir.display()
        )
    })?;
    let summary = EncryptionInventorySummary {
        empty: report.empty.len(),
        plaintext: report.plaintext.len(),
        encrypted: report.encrypted.len(),
    };
    let report = enforce_uniform_state(report, cfg.enabled).map_err(|e| {
        anyhow::anyhow!(
            "ERR_ENCRYPTION_BOOT: data directory {} failed the EaR invariant check: {e}",
            data_dir.display()
        )
    })?;
    // Drop `report` after enforcement; we only ship the counts.
    drop(report);
    Ok(EncryptionConfig {
        inventory: Some(summary),
        ..cfg
    })
}

/// Boot-time KMS provider resolution.
///
/// Reads `NEXUS_KMS_PROVIDER` and the matching `NEXUS_KMS_<NAME>_*`
/// configuration env vars, constructs the appropriate adapter from
/// `nexus_core::storage::crypto::kms`, and resolves the master key.
///
/// The function is split off so the `provider` switch lives in one
/// place; each branch errors out when the matching `kms-*` Cargo
/// feature was not enabled at build time, surfacing a precise
/// build-mismatch message instead of an opaque "unknown provider".
fn resolve_kms(
    provider: &str,
) -> anyhow::Result<(
    EncryptionSource,
    [u8; nexus_core::storage::crypto::MASTER_KEY_LEN],
)> {
    // `label()` / `master_key()` resolve through the dyn vtable
    // without needing the trait in scope.
    let provider_lower = provider.trim().to_ascii_lowercase();
    match provider_lower.as_str() {
        "aws" => resolve_kms_aws(),
        "gcp" => resolve_kms_gcp(),
        "vault" => resolve_kms_vault(),
        other => Err(anyhow::anyhow!(
            "unknown NEXUS_KMS_PROVIDER `{other}` (expected one of: aws, gcp, vault)"
        )),
    }
    .and_then(|provider| {
        let label = provider.label().to_string();
        let key = provider
            .master_key()
            .map_err(|e| anyhow::anyhow!("kms master_key resolution failed: {e}"))?;
        let source = EncryptionSource::Kms {
            provider: provider_lower,
            label,
        };
        Ok((source, *key))
    })
}

#[cfg(feature = "kms-aws")]
fn resolve_kms_aws() -> anyhow::Result<Box<dyn nexus_core::storage::crypto::KeyProvider>> {
    use nexus_core::storage::crypto::kms::{AwsKmsConfig, AwsKmsKeyProvider};
    let wrapped = std::env::var("NEXUS_KMS_WRAPPED_DEK_FILE").map_err(|_| {
        anyhow::anyhow!("NEXUS_KMS_WRAPPED_DEK_FILE is required when NEXUS_KMS_PROVIDER=aws")
    })?;
    let cfg = AwsKmsConfig {
        region: std::env::var("NEXUS_KMS_AWS_REGION").ok(),
        key_id: std::env::var("NEXUS_KMS_AWS_KEY_ID").ok(),
        wrapped_dek_path: std::path::PathBuf::from(wrapped),
        endpoint_url: std::env::var("NEXUS_KMS_AWS_ENDPOINT").ok(),
    };
    let provider = AwsKmsKeyProvider::from_config(cfg)
        .map_err(|e| anyhow::anyhow!("aws kms boot failed: {e}"))?;
    Ok(Box::new(provider))
}

#[cfg(not(feature = "kms-aws"))]
fn resolve_kms_aws() -> anyhow::Result<Box<dyn nexus_core::storage::crypto::KeyProvider>> {
    Err(anyhow::anyhow!(
        "NEXUS_KMS_PROVIDER=aws but nexus-core was built without the `kms-aws` feature"
    ))
}

#[cfg(feature = "kms-gcp")]
fn resolve_kms_gcp() -> anyhow::Result<Box<dyn nexus_core::storage::crypto::KeyProvider>> {
    use nexus_core::storage::crypto::kms::{GcpKmsConfig, GcpKmsKeyProvider};
    let wrapped = std::env::var("NEXUS_KMS_WRAPPED_DEK_FILE").map_err(|_| {
        anyhow::anyhow!("NEXUS_KMS_WRAPPED_DEK_FILE is required when NEXUS_KMS_PROVIDER=gcp")
    })?;
    let key_name = std::env::var("NEXUS_KMS_GCP_KEY_NAME").map_err(|_| {
        anyhow::anyhow!("NEXUS_KMS_GCP_KEY_NAME is required when NEXUS_KMS_PROVIDER=gcp")
    })?;
    let cfg = GcpKmsConfig {
        key_name,
        wrapped_dek_path: std::path::PathBuf::from(wrapped),
        endpoint_url: std::env::var("NEXUS_KMS_GCP_ENDPOINT").ok(),
    };
    let provider = GcpKmsKeyProvider::from_config(cfg)
        .map_err(|e| anyhow::anyhow!("gcp kms boot failed: {e}"))?;
    Ok(Box::new(provider))
}

#[cfg(not(feature = "kms-gcp"))]
fn resolve_kms_gcp() -> anyhow::Result<Box<dyn nexus_core::storage::crypto::KeyProvider>> {
    Err(anyhow::anyhow!(
        "NEXUS_KMS_PROVIDER=gcp but nexus-core was built without the `kms-gcp` feature"
    ))
}

#[cfg(feature = "kms-vault")]
fn resolve_kms_vault() -> anyhow::Result<Box<dyn nexus_core::storage::crypto::KeyProvider>> {
    use nexus_core::storage::crypto::kms::{VaultConfig, VaultKeyProvider};
    let address = std::env::var("NEXUS_KMS_VAULT_ADDR").map_err(|_| {
        anyhow::anyhow!("NEXUS_KMS_VAULT_ADDR is required when NEXUS_KMS_PROVIDER=vault")
    })?;
    let token = std::env::var("NEXUS_KMS_VAULT_TOKEN").map_err(|_| {
        anyhow::anyhow!("NEXUS_KMS_VAULT_TOKEN is required when NEXUS_KMS_PROVIDER=vault")
    })?;
    let mount = std::env::var("NEXUS_KMS_VAULT_MOUNT").unwrap_or_else(|_| "transit".to_string());
    let key_name = std::env::var("NEXUS_KMS_VAULT_KEY").map_err(|_| {
        anyhow::anyhow!("NEXUS_KMS_VAULT_KEY is required when NEXUS_KMS_PROVIDER=vault")
    })?;
    let wrapped = std::env::var("NEXUS_KMS_WRAPPED_DEK_FILE").map_err(|_| {
        anyhow::anyhow!("NEXUS_KMS_WRAPPED_DEK_FILE is required when NEXUS_KMS_PROVIDER=vault")
    })?;
    let cfg = VaultConfig {
        address,
        token,
        namespace: std::env::var("NEXUS_KMS_VAULT_NAMESPACE").ok(),
        mount,
        key_name,
        wrapped_dek_path: std::path::PathBuf::from(wrapped),
        insecure_skip_verify: std::env::var("NEXUS_KMS_VAULT_INSECURE_SKIP_VERIFY")
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
            .unwrap_or(false),
    };
    let provider = VaultKeyProvider::from_config(cfg)
        .map_err(|e| anyhow::anyhow!("vault kms boot failed: {e}"))?;
    Ok(Box::new(provider))
}

#[cfg(not(feature = "kms-vault"))]
fn resolve_kms_vault() -> anyhow::Result<Box<dyn nexus_core::storage::crypto::KeyProvider>> {
    Err(anyhow::anyhow!(
        "NEXUS_KMS_PROVIDER=vault but nexus-core was built without the `kms-vault` feature"
    ))
}

/// SHA-256 fingerprint of the master key — first 16 hex digits of
/// the digest, prefixed with `nexus:` for log readability. Short
/// enough to fit in a log line, long enough that birthday-collisions
/// against a hostile observer are negligible (`2^32` keys).
pub fn fingerprint_master_key(key: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"nexus-master-key-fingerprint-v1");
    hasher.update(key);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(22);
    out.push_str("nexus:");
    for byte in &digest[..8] {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod encryption_tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_for_the_same_key() {
        let k = [0x42u8; 32];
        let a = fingerprint_master_key(&k);
        let b = fingerprint_master_key(&k);
        assert_eq!(a, b);
        assert!(a.starts_with("nexus:"));
        assert_eq!(a.len(), "nexus:".len() + 16);
    }

    #[test]
    fn fingerprint_changes_with_the_key() {
        let a = fingerprint_master_key(&[0u8; 32]);
        let b = fingerprint_master_key(&[1u8; 32]);
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_does_not_leak_key_bytes() {
        let k = [0xAAu8; 32];
        let fp = fingerprint_master_key(&k);
        // Naive check: the literal hex of the key must not appear
        // in the fingerprint output.
        assert!(!fp.contains("aaaaaa"), "fingerprint leaked key bytes");
    }

    #[test]
    fn resolve_disabled_returns_default() {
        // Use a unique env var so parallel tests don't collide.
        unsafe { std::env::remove_var("NEXUS_ENCRYPT_AT_REST") };
        let cfg = resolve_encryption_config().expect("resolve");
        assert!(!cfg.enabled);
        assert!(cfg.source.is_none());
        assert!(cfg.fingerprint.is_none());
    }

    #[test]
    fn resolve_with_env_key_records_source_and_fingerprint() {
        unsafe { std::env::set_var("NEXUS_ENCRYPT_AT_REST", "true") };
        unsafe { std::env::set_var("NEXUS_DATA_KEY", "a".repeat(64)) };
        unsafe { std::env::remove_var("NEXUS_KEY_FILE") };
        let cfg = resolve_encryption_config().expect("resolve");
        assert!(cfg.enabled);
        assert_eq!(
            cfg.source,
            Some(EncryptionSource::Env {
                name: "NEXUS_DATA_KEY".into()
            })
        );
        assert!(cfg.fingerprint.unwrap().starts_with("nexus:"));
        unsafe {
            std::env::remove_var("NEXUS_ENCRYPT_AT_REST");
            std::env::remove_var("NEXUS_DATA_KEY");
        }
    }

    #[test]
    fn resolve_rejects_bad_key_format() {
        unsafe { std::env::set_var("NEXUS_ENCRYPT_AT_REST", "true") };
        // 8-byte key — invalid; expect 32 raw or 64 hex.
        unsafe { std::env::set_var("NEXUS_DATA_KEY", "shortkey") };
        unsafe { std::env::remove_var("NEXUS_KEY_FILE") };
        let err = resolve_encryption_config().unwrap_err();
        assert!(err.to_string().contains("NEXUS_DATA_KEY"));
        unsafe {
            std::env::remove_var("NEXUS_ENCRYPT_AT_REST");
            std::env::remove_var("NEXUS_DATA_KEY");
        }
    }

    #[test]
    fn resolve_rejects_unknown_kms_provider() {
        unsafe {
            std::env::set_var("NEXUS_ENCRYPT_AT_REST", "true");
            std::env::set_var("NEXUS_KMS_PROVIDER", "rot13");
            std::env::remove_var("NEXUS_KEY_FILE");
            std::env::remove_var("NEXUS_DATA_KEY");
        }
        let err = resolve_encryption_config().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("rot13"), "got {msg}");
        assert!(
            msg.contains("aws"),
            "expected the error to enumerate providers, got {msg}"
        );
        unsafe {
            std::env::remove_var("NEXUS_ENCRYPT_AT_REST");
            std::env::remove_var("NEXUS_KMS_PROVIDER");
        }
    }

    #[cfg(not(feature = "kms-aws"))]
    #[test]
    fn resolve_kms_aws_without_feature_surfaces_clear_error() {
        unsafe {
            std::env::set_var("NEXUS_ENCRYPT_AT_REST", "true");
            std::env::set_var("NEXUS_KMS_PROVIDER", "aws");
            std::env::remove_var("NEXUS_KEY_FILE");
            std::env::remove_var("NEXUS_DATA_KEY");
        }
        let err = resolve_encryption_config().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("kms-aws"), "got {msg}");
        unsafe {
            std::env::remove_var("NEXUS_ENCRYPT_AT_REST");
            std::env::remove_var("NEXUS_KMS_PROVIDER");
        }
    }

    #[test]
    fn enforce_invariants_on_empty_dir_returns_zero_summary() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = EncryptionConfig::default();
        let out = enforce_data_dir_invariants(cfg, dir.path()).expect("clean dir");
        let inv = out.inventory.expect("inventory recorded");
        assert_eq!(inv.empty, 0);
        assert_eq!(inv.plaintext, 0);
        assert_eq!(inv.encrypted, 0);
    }

    #[test]
    fn enforce_invariants_rejects_mixed_mode_dir() {
        use nexus_core::storage::crypto::{FileId, PageHeader};
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        // Plaintext file: 16 zero bytes — magic mismatch.
        std::fs::File::create(dir.path().join("plain.bin"))
            .unwrap()
            .write_all(&[0u8; 16])
            .unwrap();
        // Encrypted file: valid first-page header.
        let header = PageHeader {
            file_id: FileId::Catalog,
            generation: 0,
        }
        .to_bytes();
        std::fs::File::create(dir.path().join("enc.bin"))
            .unwrap()
            .write_all(&header)
            .unwrap();
        let cfg = EncryptionConfig::default();
        let err = enforce_data_dir_invariants(cfg, dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("ERR_ENCRYPTION_BOOT"), "got {msg}");
        assert!(msg.contains("MIXED_MODE"), "got {msg}");
    }

    #[test]
    fn enforce_invariants_rejects_encrypted_dir_when_disabled() {
        use nexus_core::storage::crypto::{FileId, PageHeader};
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let header = PageHeader {
            file_id: FileId::Wal,
            generation: 1,
        }
        .to_bytes();
        std::fs::File::create(dir.path().join("wal.bin"))
            .unwrap()
            .write_all(&header)
            .unwrap();
        let cfg = EncryptionConfig::default(); // enabled = false
        let err = enforce_data_dir_invariants(cfg, dir.path()).unwrap_err();
        assert!(err.to_string().contains("UNEXPECTED_ENCRYPTED"));
    }

    #[test]
    fn enforce_invariants_records_summary_on_success() {
        use nexus_core::storage::crypto::{FileId, PageHeader};
        use std::io::Write;
        let dir = tempfile::TempDir::new().unwrap();
        let header = PageHeader {
            file_id: FileId::Catalog,
            generation: 0,
        }
        .to_bytes();
        std::fs::File::create(dir.path().join("a.bin"))
            .unwrap()
            .write_all(&header)
            .unwrap();
        std::fs::File::create(dir.path().join("b.bin"))
            .unwrap()
            .write_all(&header)
            .unwrap();
        let cfg = EncryptionConfig {
            enabled: true,
            ..Default::default()
        };
        let out = enforce_data_dir_invariants(cfg, dir.path()).expect("uniform encrypted");
        let inv = out.inventory.expect("inventory");
        assert_eq!(inv.encrypted, 2);
        assert_eq!(inv.plaintext, 0);
    }

    #[test]
    fn encryption_source_kms_serialises_with_kind_tag() {
        let src = EncryptionSource::Kms {
            provider: "vault".into(),
            label: "vault:http://x/transit/k".into(),
        };
        let json = serde_json::to_string(&src).unwrap();
        // `serde(tag = "kind", rename_all = "snake_case")` on the
        // enum means each variant ships with a "kind" discriminator.
        // The /admin/encryption/status response depends on this
        // shape; locking it in here so an accidental tag rename
        // surfaces as a test failure rather than a silent client
        // break.
        assert!(json.contains("\"kind\":\"kms\""), "got {json}");
        assert!(json.contains("\"provider\":\"vault\""), "got {json}");
        assert!(json.contains("vault:http://x/transit/k"), "got {json}");
    }
}
