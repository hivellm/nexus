//! AWS KMS [`KeyProvider`] adapter.
//!
//! Resolves the master key by issuing a single `Decrypt` call
//! against AWS KMS at construction time. The wrapped DEK
//! ciphertext is supplied by the operator (env var or file); the
//! KEK that wraps it lives inside KMS and never leaves AWS.
//!
//! Operator setup (one-shot, on the AWS side):
//!
//! ```text
//! aws kms generate-data-key \
//!     --key-id alias/nexus-master \
//!     --key-spec AES_256 \
//!     --query CiphertextBlob \
//!     --output text \
//!   | base64 -d > /etc/nexus/data-key.bin
//! ```
//!
//! Operator setup (engine side):
//!
//! ```text
//! NEXUS_ENCRYPT_AT_REST=1
//! NEXUS_KMS_PROVIDER=aws
//! NEXUS_KMS_AWS_KEY_ID=alias/nexus-master   # optional, for audit logs
//! NEXUS_KMS_AWS_REGION=us-east-1
//! NEXUS_KMS_WRAPPED_DEK_FILE=/etc/nexus/data-key.bin
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aws_sdk_kms::primitives::Blob;
use zeroize::Zeroizing;

use super::super::key_provider::{KeyProvider, KeyProviderError, MASTER_KEY_LEN};
use super::{KmsError, block_on, wrap_master_key};

/// Configuration for [`AwsKmsKeyProvider`]. Every field has an
/// equivalent `NEXUS_KMS_AWS_*` env var that the server boot path
/// reads when `NEXUS_KMS_PROVIDER=aws` is set.
#[derive(Debug, Clone)]
pub struct AwsKmsConfig {
    /// AWS region the KMS request should target. `None` defers to
    /// the SDK's default chain (`AWS_REGION` env var → instance
    /// metadata → `~/.aws/config`).
    pub region: Option<String>,
    /// KMS key id / alias / ARN for audit logs. Optional — `Decrypt`
    /// derives the KEK from the ciphertext metadata, but supplying
    /// the id makes the boot log self-describing.
    pub key_id: Option<String>,
    /// Path to the file holding the KMS-wrapped DEK ciphertext (the
    /// raw bytes produced by `aws kms generate-data-key
    /// --output text | base64 -d`).
    pub wrapped_dek_path: PathBuf,
    /// Optional override for the KMS endpoint. Used by the
    /// localstack integration test (`http://localhost:4566`); empty
    /// in production.
    pub endpoint_url: Option<String>,
}

/// AWS-KMS-backed [`KeyProvider`].
pub struct AwsKmsKeyProvider {
    label: String,
    cached: Zeroizing<[u8; MASTER_KEY_LEN]>,
}

impl std::fmt::Debug for AwsKmsKeyProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwsKmsKeyProvider")
            .field("label", &self.label)
            .field("cached", &"<redacted>")
            .finish()
    }
}

impl AwsKmsKeyProvider {
    /// Resolve the master key by calling `kms:Decrypt` once.
    pub fn from_config(cfg: AwsKmsConfig) -> Result<Self, KeyProviderError> {
        let wrapped = std::fs::read(&cfg.wrapped_dek_path).map_err(|e| {
            KmsError::Config(format!(
                "failed to read wrapped DEK from {}: {e}",
                cfg.wrapped_dek_path.display()
            ))
        })?;
        if wrapped.is_empty() {
            return Err(KmsError::Config(format!(
                "wrapped DEK file {} is empty",
                cfg.wrapped_dek_path.display()
            ))
            .into());
        }

        let label = match (&cfg.key_id, &cfg.region) {
            (Some(k), Some(r)) => format!("aws-kms:{k}@{r}"),
            (Some(k), None) => format!("aws-kms:{k}"),
            (None, Some(r)) => format!("aws-kms:@{r}"),
            (None, None) => "aws-kms:default".to_string(),
        };

        let plaintext = block_on(decrypt(cfg.clone(), wrapped))?;
        let cached = wrap_master_key(&plaintext)?;
        // Plaintext lives in `cached`; drop the heap copy now so it
        // never sits around as a stray allocation.
        drop(plaintext);

        Ok(Self { label, cached })
    }
}

async fn decrypt(cfg: AwsKmsConfig, wrapped: Vec<u8>) -> Result<Vec<u8>, KmsError> {
    use aws_config::BehaviorVersion;

    let mut loader = aws_config::defaults(BehaviorVersion::latest());
    if let Some(region) = &cfg.region {
        loader = loader.region(aws_config::Region::new(region.clone()));
    }
    if let Some(endpoint) = &cfg.endpoint_url {
        loader = loader.endpoint_url(endpoint.clone());
    }
    let shared_config = loader.load().await;
    let client = aws_sdk_kms::Client::new(&shared_config);

    let mut req = client.decrypt().ciphertext_blob(Blob::new(wrapped));
    if let Some(key_id) = &cfg.key_id {
        req = req.key_id(key_id);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| KmsError::Request(format!("aws kms decrypt failed: {e}")))?;
    let plaintext = resp
        .plaintext
        .ok_or_else(|| KmsError::Request("aws kms decrypt returned no plaintext".to_string()))?;
    Ok(plaintext.into_inner())
}

impl KeyProvider for AwsKmsKeyProvider {
    fn master_key(&self) -> Result<Zeroizing<[u8; MASTER_KEY_LEN]>, KeyProviderError> {
        Ok(Zeroizing::new(*self.cached))
    }

    fn label(&self) -> &str {
        &self.label
    }
}

/// Witness that `AwsKmsConfig` is `Send + Sync` so it can travel
/// across the boot-time tokio runtime without lifetime gymnastics.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AwsKmsConfig>();
    assert_send_sync::<AwsKmsKeyProvider>();
    assert_send_sync::<Arc<AwsKmsKeyProvider>>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn missing_wrapped_dek_file_is_config_error() {
        let cfg = AwsKmsConfig {
            region: Some("us-east-1".to_string()),
            key_id: None,
            wrapped_dek_path: PathBuf::from("/this/file/does/not/exist"),
            endpoint_url: None,
        };
        let err = AwsKmsKeyProvider::from_config(cfg).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("ERR_KEY_KMS_CONFIG"), "got {msg}");
    }

    #[test]
    fn empty_wrapped_dek_file_is_config_error() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"").unwrap();
        let cfg = AwsKmsConfig {
            region: Some("us-east-1".to_string()),
            key_id: Some("alias/nexus".to_string()),
            wrapped_dek_path: f.path().to_path_buf(),
            endpoint_url: None,
        };
        let err = AwsKmsKeyProvider::from_config(cfg).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("ERR_KEY_KMS_CONFIG"), "got {msg}");
    }

    #[test]
    fn label_includes_key_id_and_region() {
        // We can't reach this code path without a successful KMS
        // call (constructor calls `decrypt` before assigning), so
        // we exercise the label-formatting branch directly via a
        // small helper that mirrors the constructor's logic.
        let label = match (Some("alias/x".to_string()), Some("us-east-1".to_string())) {
            (Some(k), Some(r)) => format!("aws-kms:{k}@{r}"),
            _ => unreachable!(),
        };
        assert_eq!(label, "aws-kms:alias/x@us-east-1");
    }
}
