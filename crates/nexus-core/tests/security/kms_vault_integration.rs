//! HashiCorp Vault transit-engine integration test (`vault dev`).
//!
//! Run manually:
//!
//! ```bash
//! # Start a dev Vault server (root token = `dev-token`):
//! vault server -dev -dev-root-token-id=dev-token \
//!     -dev-listen-address=127.0.0.1:8200 &
//!
//! export VAULT_ADDR=http://127.0.0.1:8200
//! export VAULT_TOKEN=dev-token
//!
//! # Enable transit + create a key + wrap a 32-byte DEK:
//! vault secrets enable transit
//! vault write -f transit/keys/nexus-master
//! vault write transit/encrypt/nexus-master \
//!     plaintext=$(head -c 32 /dev/urandom | base64) \
//!     -format=json | jq -r .data.ciphertext > /tmp/wrapped-dek.txt
//!
//! NEXUS_KMS_VAULT_TEST_ADDR=http://127.0.0.1:8200 \
//! NEXUS_KMS_VAULT_TEST_TOKEN=dev-token \
//! NEXUS_KMS_VAULT_TEST_KEY=nexus-master \
//! NEXUS_KMS_VAULT_TEST_DEK=/tmp/wrapped-dek.txt \
//!     cargo test --features kms-vault --test kms_vault_integration -- --ignored --nocapture
//! ```
//!
//! Skipped by default. `vault dev` is the canonical local mock
//! and is not bundled with CI by default.

#![cfg(feature = "kms-vault")]

use std::path::PathBuf;

use nexus_core::storage::crypto::kms::{VaultConfig, VaultKeyProvider};
use nexus_core::storage::crypto::{KeyProvider, MASTER_KEY_LEN};

#[test]
#[ignore = "requires `vault dev` at NEXUS_KMS_VAULT_TEST_ADDR"]
fn vault_kms_unwraps_dek_against_vault_dev() {
    let address =
        std::env::var("NEXUS_KMS_VAULT_TEST_ADDR").expect("set NEXUS_KMS_VAULT_TEST_ADDR");
    let token =
        std::env::var("NEXUS_KMS_VAULT_TEST_TOKEN").expect("set NEXUS_KMS_VAULT_TEST_TOKEN");
    let key_name = std::env::var("NEXUS_KMS_VAULT_TEST_KEY").expect("set NEXUS_KMS_VAULT_TEST_KEY");
    let dek_path = std::env::var("NEXUS_KMS_VAULT_TEST_DEK").expect("set NEXUS_KMS_VAULT_TEST_DEK");

    let cfg = VaultConfig {
        address: address.clone(),
        token,
        namespace: None,
        mount: "transit".to_string(),
        key_name: key_name.clone(),
        wrapped_dek_path: PathBuf::from(dek_path),
        insecure_skip_verify: true,
    };
    let provider = VaultKeyProvider::from_config(cfg).expect("vault boot");
    let key = provider.master_key().expect("master key resolution");
    assert_eq!(key.len(), MASTER_KEY_LEN);
    let label = provider.label();
    assert!(label.contains(&address), "label = {label}");
    assert!(label.contains(&key_name), "label = {label}");
}
