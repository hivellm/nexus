# DEK pattern + feature-gated SDK adapters for managed-KMS KeyProviders

**Category**: security
**Tags**: encryption-at-rest, kms, key-management, feature-gating

## Description

Cloud-KMS adapters for `KeyProvider` use the data-encryption-key (DEK) pattern: KMS holds the KEK, the operator stores the wrapped DEK on disk, the adapter unwraps the DEK once at construction and caches the 32-byte plaintext in `Zeroizing<[u8; 32]>`. The KMS is never on the hot path. Each adapter lives behind its own Cargo feature so default builds avoid the SDK transitive-dep cost.

## Example

// crates/nexus-core/src/storage/crypto/kms/aws.rs (template)
pub struct AwsKmsConfig {
    pub region: Option<String>,
    pub key_id: Option<String>,
    pub wrapped_dek_path: PathBuf,
    pub endpoint_url: Option<String>,
}

impl AwsKmsKeyProvider {
    pub fn from_config(cfg: AwsKmsConfig) -> Result<Self, KeyProviderError> {
        let wrapped = std::fs::read(&cfg.wrapped_dek_path)
            .map_err(|e| KmsError::Config(format!("...: {e}")))?;
        let plaintext = block_on(decrypt(cfg.clone(), wrapped))?;
        let cached = wrap_master_key(&plaintext)?;
        Ok(Self { label, cached })
    }
}

## When to Use

Adding a new managed-KMS adapter (HSM, Azure Key Vault, etc.) — copy the AWS/GCP/Vault skeleton: a `*Config` struct with operator-supplied fields, a constructor that validates config, calls the SDK's decrypt-equivalent through `block_on`, validates the plaintext length via `wrap_master_key`, caches it. Re-export the config + provider from `kms/mod.rs`. Add a `#[cfg(feature = "kms-<name>")]` branch to `resolve_kms` in `nexus-server/src/config.rs` plus a stub for the `not(feature)` build that fails fast with a clear message.

## When NOT to Use

For self-hosted deployments where the master key is provisioned out-of-band — `EnvKeyProvider` and `FileKeyProvider` are simpler and have no transitive SDK cost.
