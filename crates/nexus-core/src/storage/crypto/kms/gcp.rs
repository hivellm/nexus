//! GCP KMS [`KeyProvider`] adapter.
//!
//! Resolves the master key by issuing one Cloud-KMS `Decrypt`
//! call. The wrapped DEK ciphertext is supplied by the operator
//! (env var or file); the KEK lives inside Cloud KMS and never
//! leaves Google.
//!
//! Operator setup (one-shot, on the GCP side):
//!
//! ```text
//! gcloud kms keys create nexus-master \
//!     --location=global \
//!     --keyring=nexus-keyring \
//!     --purpose=encryption
//!
//! head -c 32 /dev/urandom > /tmp/dek.bin
//! gcloud kms encrypt \
//!     --location=global \
//!     --keyring=nexus-keyring \
//!     --key=nexus-master \
//!     --plaintext-file=/tmp/dek.bin \
//!     --ciphertext-file=/etc/nexus/data-key.bin
//! shred -u /tmp/dek.bin
//! ```
//!
//! Operator setup (engine side):
//!
//! ```text
//! NEXUS_ENCRYPT_AT_REST=1
//! NEXUS_KMS_PROVIDER=gcp
//! NEXUS_KMS_GCP_KEY_NAME=projects/<proj>/locations/global/keyRings/nexus-keyring/cryptoKeys/nexus-master
//! NEXUS_KMS_WRAPPED_DEK_FILE=/etc/nexus/data-key.bin
//! ```
//!
//! Authentication uses the GCP default credential chain: service
//! account JSON via `GOOGLE_APPLICATION_CREDENTIALS`, GKE Workload
//! Identity, or instance metadata.

use std::path::PathBuf;

use google_cloud_kms::client::{Client, ClientConfig};
use google_cloud_kms::grpc::kms::v1::DecryptRequest;
use zeroize::Zeroizing;

use super::super::key_provider::{KeyProvider, KeyProviderError, MASTER_KEY_LEN};
use super::{KmsError, block_on, wrap_master_key};

/// Configuration for [`GcpKmsKeyProvider`].
#[derive(Debug, Clone)]
pub struct GcpKmsConfig {
    /// Fully-qualified Cloud-KMS key resource path:
    /// `projects/<p>/locations/<l>/keyRings/<r>/cryptoKeys/<k>`.
    pub key_name: String,
    /// Path to the file holding the KMS-wrapped DEK ciphertext.
    pub wrapped_dek_path: PathBuf,
    /// Optional override for the Cloud-KMS endpoint. Used by the
    /// emulator integration test (`http://localhost:9020`); empty
    /// in production.
    pub endpoint_url: Option<String>,
}

/// GCP-KMS-backed [`KeyProvider`].
pub struct GcpKmsKeyProvider {
    label: String,
    cached: Zeroizing<[u8; MASTER_KEY_LEN]>,
}

impl std::fmt::Debug for GcpKmsKeyProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GcpKmsKeyProvider")
            .field("label", &self.label)
            .field("cached", &"<redacted>")
            .finish()
    }
}

impl GcpKmsKeyProvider {
    /// Resolve the master key by calling Cloud-KMS `Decrypt` once.
    pub fn from_config(cfg: GcpKmsConfig) -> Result<Self, KeyProviderError> {
        if cfg.key_name.is_empty() {
            return Err(KmsError::Config("gcp key_name is empty".into()).into());
        }
        if !cfg.key_name.contains("/cryptoKeys/") {
            return Err(KmsError::Config(format!(
                "gcp key_name must be a fully-qualified cryptoKey path (got {})",
                cfg.key_name
            ))
            .into());
        }

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

        let label = format!("gcp-kms:{}", cfg.key_name);
        let plaintext = block_on(decrypt(cfg, wrapped))?;
        let cached = wrap_master_key(&plaintext)?;
        drop(plaintext);

        Ok(Self { label, cached })
    }
}

async fn decrypt(cfg: GcpKmsConfig, wrapped: Vec<u8>) -> Result<Vec<u8>, KmsError> {
    let mut client_config = ClientConfig::default()
        .with_auth()
        .await
        .map_err(|e| KmsError::Config(format!("gcp auth init failed: {e}")))?;
    if let Some(endpoint) = cfg.endpoint_url {
        client_config.endpoint = endpoint;
    }
    let client = Client::new(client_config)
        .await
        .map_err(|e| KmsError::Request(format!("gcp kms client init failed: {e}")))?;

    let req = DecryptRequest {
        name: cfg.key_name.clone(),
        ciphertext: wrapped,
        ciphertext_crc32c: None,
        additional_authenticated_data: vec![],
        additional_authenticated_data_crc32c: None,
    };
    let resp = client
        .decrypt(req, None)
        .await
        .map_err(|e| KmsError::Request(format!("gcp kms decrypt failed: {e}")))?;
    Ok(resp.plaintext)
}

impl KeyProvider for GcpKmsKeyProvider {
    fn master_key(&self) -> Result<Zeroizing<[u8; MASTER_KEY_LEN]>, KeyProviderError> {
        Ok(Zeroizing::new(*self.cached))
    }

    fn label(&self) -> &str {
        &self.label
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn cfg(path: PathBuf) -> GcpKmsConfig {
        GcpKmsConfig {
            key_name: "projects/p/locations/global/keyRings/r/cryptoKeys/k".to_string(),
            wrapped_dek_path: path,
            endpoint_url: None,
        }
    }

    #[test]
    fn empty_key_name_is_config_error() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"ciphertext").unwrap();
        let mut c = cfg(f.path().to_path_buf());
        c.key_name.clear();
        let err = GcpKmsKeyProvider::from_config(c).unwrap_err();
        assert!(format!("{err}").contains("ERR_KEY_KMS_CONFIG"));
    }

    #[test]
    fn malformed_key_name_is_config_error() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"ciphertext").unwrap();
        let mut c = cfg(f.path().to_path_buf());
        c.key_name = "not-a-resource-path".to_string();
        let err = GcpKmsKeyProvider::from_config(c).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("ERR_KEY_KMS_CONFIG"));
        assert!(msg.contains("cryptoKey"));
    }

    #[test]
    fn missing_wrapped_dek_file_is_config_error() {
        let c = cfg(PathBuf::from("/this/path/does/not/exist"));
        let err = GcpKmsKeyProvider::from_config(c).unwrap_err();
        assert!(format!("{err}").contains("ERR_KEY_KMS_CONFIG"));
    }

    #[test]
    fn empty_wrapped_dek_file_is_config_error() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"").unwrap();
        let c = cfg(f.path().to_path_buf());
        let err = GcpKmsKeyProvider::from_config(c).unwrap_err();
        assert!(format!("{err}").contains("ERR_KEY_KMS_CONFIG"));
    }
}
