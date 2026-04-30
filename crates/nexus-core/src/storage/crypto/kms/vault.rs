//! HashiCorp Vault [`KeyProvider`] adapter (transit secret engine).
//!
//! Uses Vault's transit engine `decrypt` endpoint as the unwrap
//! call. The wrapped DEK is the operator-supplied transit
//! ciphertext (the `vault:v<n>:...` blob produced by `vault write
//! transit/encrypt/<key>`); the KEK lives inside Vault and never
//! leaves it.
//!
//! Operator setup (one-shot, on the Vault side):
//!
//! ```text
//! vault secrets enable transit
//! vault write -f transit/keys/nexus-master
//! vault write transit/encrypt/nexus-master \
//!     plaintext=$(head -c 32 /dev/urandom | base64) \
//!   | tee /etc/nexus/wrapped-dek.txt    # captures `vault:v1:...`
//! ```
//!
//! Operator setup (engine side):
//!
//! ```text
//! NEXUS_ENCRYPT_AT_REST=1
//! NEXUS_KMS_PROVIDER=vault
//! NEXUS_KMS_VAULT_ADDR=https://vault.example.com:8200
//! NEXUS_KMS_VAULT_TOKEN=...               # OR namespace + auth method
//! NEXUS_KMS_VAULT_MOUNT=transit
//! NEXUS_KMS_VAULT_KEY=nexus-master
//! NEXUS_KMS_WRAPPED_DEK_FILE=/etc/nexus/wrapped-dek.txt
//! ```

use std::path::PathBuf;

use base64::Engine as _;
use vaultrs::client::{VaultClient, VaultClientSettingsBuilder};
use zeroize::Zeroizing;

use super::super::key_provider::{KeyProvider, KeyProviderError, MASTER_KEY_LEN};
use super::{KmsError, block_on, wrap_master_key};

/// Configuration for [`VaultKeyProvider`].
#[derive(Debug, Clone)]
pub struct VaultConfig {
    /// Vault address, e.g. `https://vault.example.com:8200`.
    pub address: String,
    /// Auth token. The adapter does not issue `auth/login` itself —
    /// operators are expected to mint a token out-of-band (CLI,
    /// AppRole helper, sidecar) and inject it via env / config.
    pub token: String,
    /// Optional Vault namespace (Enterprise feature). `None` for
    /// open-source Vault.
    pub namespace: Option<String>,
    /// Mount point of the transit engine. Conventionally `transit`.
    pub mount: String,
    /// Transit key name.
    pub key_name: String,
    /// Path to the file holding the transit ciphertext
    /// (`vault:v<n>:...`).
    pub wrapped_dek_path: PathBuf,
    /// Optional flag — disable TLS verification. Keep `false` in
    /// production; the integration test against `vault dev`
    /// flips this to `true` because the dev server self-signs.
    pub insecure_skip_verify: bool,
}

/// Vault-transit-backed [`KeyProvider`].
pub struct VaultKeyProvider {
    label: String,
    cached: Zeroizing<[u8; MASTER_KEY_LEN]>,
}

impl std::fmt::Debug for VaultKeyProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultKeyProvider")
            .field("label", &self.label)
            .field("cached", &"<redacted>")
            .finish()
    }
}

impl VaultKeyProvider {
    /// Resolve the master key by issuing one transit-engine
    /// `decrypt` call.
    pub fn from_config(cfg: VaultConfig) -> Result<Self, KeyProviderError> {
        if cfg.address.is_empty() {
            return Err(KmsError::Config("vault address is empty".into()).into());
        }
        if cfg.token.is_empty() {
            return Err(KmsError::Config("vault token is empty".into()).into());
        }
        if cfg.mount.is_empty() {
            return Err(KmsError::Config("vault mount is empty".into()).into());
        }
        if cfg.key_name.is_empty() {
            return Err(KmsError::Config("vault key name is empty".into()).into());
        }

        let ciphertext_raw = std::fs::read(&cfg.wrapped_dek_path).map_err(|e| {
            KmsError::Config(format!(
                "failed to read wrapped DEK from {}: {e}",
                cfg.wrapped_dek_path.display()
            ))
        })?;
        let ciphertext = String::from_utf8(ciphertext_raw)
            .map_err(|e| KmsError::Config(format!("wrapped DEK is not utf-8: {e}")))?;
        let ciphertext = ciphertext.trim().to_string();
        if !ciphertext.starts_with("vault:") {
            return Err(KmsError::Config(format!(
                "wrapped DEK does not look like a transit ciphertext (expected `vault:v<n>:...`, got {} bytes)",
                ciphertext.len()
            ))
            .into());
        }

        let label = format!(
            "vault:{addr}/{mount}/{key}",
            addr = cfg.address,
            mount = cfg.mount,
            key = cfg.key_name
        );

        let plaintext = block_on(decrypt(cfg, ciphertext))?;
        let cached = wrap_master_key(&plaintext)?;
        drop(plaintext);

        Ok(Self { label, cached })
    }
}

async fn decrypt(cfg: VaultConfig, ciphertext: String) -> Result<Vec<u8>, KmsError> {
    let mut builder = VaultClientSettingsBuilder::default();
    builder
        .address(cfg.address.clone())
        .token(cfg.token.clone())
        .verify(!cfg.insecure_skip_verify);
    if let Some(ns) = cfg.namespace.clone() {
        builder.namespace(Some(ns));
    }
    let settings = builder
        .build()
        .map_err(|e| KmsError::Config(format!("invalid vault settings: {e}")))?;
    let client = VaultClient::new(settings)
        .map_err(|e| KmsError::Config(format!("failed to construct vault client: {e}")))?;

    let resp =
        vaultrs::transit::data::decrypt(&client, &cfg.mount, &cfg.key_name, &ciphertext, None)
            .await
            .map_err(|e| KmsError::Request(format!("vault transit decrypt failed: {e}")))?;

    // Vault returns the plaintext as a base64-encoded string.
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(resp.plaintext.as_bytes())
        .map_err(|e| KmsError::Request(format!("vault transit plaintext is not base64: {e}")))?;
    Ok(decoded)
}

impl KeyProvider for VaultKeyProvider {
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

    fn cfg(path: PathBuf) -> VaultConfig {
        VaultConfig {
            address: "http://127.0.0.1:8200".to_string(),
            token: "dev-token".to_string(),
            namespace: None,
            mount: "transit".to_string(),
            key_name: "nexus".to_string(),
            wrapped_dek_path: path,
            insecure_skip_verify: true,
        }
    }

    #[test]
    fn empty_address_is_config_error() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"vault:v1:abc").unwrap();
        let mut c = cfg(f.path().to_path_buf());
        c.address.clear();
        let err = VaultKeyProvider::from_config(c).unwrap_err();
        assert!(format!("{err}").contains("ERR_KEY_KMS_CONFIG"));
    }

    #[test]
    fn empty_token_is_config_error() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"vault:v1:abc").unwrap();
        let mut c = cfg(f.path().to_path_buf());
        c.token.clear();
        let err = VaultKeyProvider::from_config(c).unwrap_err();
        assert!(format!("{err}").contains("ERR_KEY_KMS_CONFIG"));
    }

    #[test]
    fn empty_mount_is_config_error() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"vault:v1:abc").unwrap();
        let mut c = cfg(f.path().to_path_buf());
        c.mount.clear();
        let err = VaultKeyProvider::from_config(c).unwrap_err();
        assert!(format!("{err}").contains("ERR_KEY_KMS_CONFIG"));
    }

    #[test]
    fn missing_wrapped_dek_file_is_config_error() {
        let c = cfg(PathBuf::from("/this/path/does/not/exist"));
        let err = VaultKeyProvider::from_config(c).unwrap_err();
        assert!(format!("{err}").contains("ERR_KEY_KMS_CONFIG"));
    }

    #[test]
    fn malformed_ciphertext_is_config_error() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"not-a-vault-blob").unwrap();
        let c = cfg(f.path().to_path_buf());
        let err = VaultKeyProvider::from_config(c).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("ERR_KEY_KMS_CONFIG"), "got {msg}");
        assert!(msg.contains("transit ciphertext"), "got {msg}");
    }
}
