//! GCP KMS integration test (cloud-kms emulator).
//!
//! Run manually:
//!
//! ```bash
//! # Start a local GCP KMS emulator (community-maintained image):
//! docker run --rm -d --name kms-emulator -p 9020:9020 \
//!     ghcr.io/oxidecomputer/google-cloud-kms-emulator:latest
//!
//! # Provision a key + wrap a 32-byte DEK using `gcloud` against
//! # the emulator (the emulator accepts any auth token).
//! head -c 32 /dev/urandom > /tmp/dek.bin
//! gcloud kms encrypt --plaintext-file=/tmp/dek.bin \
//!     --ciphertext-file=/tmp/wrapped.bin \
//!     --location=global --keyring=test --key=master \
//!     --project=test
//!
//! NEXUS_KMS_GCP_TEST_ENDPOINT=http://localhost:9020 \
//! NEXUS_KMS_GCP_TEST_KEY=projects/test/locations/global/keyRings/test/cryptoKeys/master \
//! NEXUS_KMS_GCP_TEST_DEK=/tmp/wrapped.bin \
//!     cargo test --features kms-gcp --test kms_gcp_integration -- --ignored --nocapture
//! ```
//!
//! Skipped by default. The emulator is not part of the standard
//! CI image set; the test runs locally on demand.

#![cfg(feature = "kms-gcp")]

use std::path::PathBuf;

use nexus_core::storage::crypto::kms::{GcpKmsConfig, GcpKmsKeyProvider};
use nexus_core::storage::crypto::{KeyProvider, MASTER_KEY_LEN};

#[test]
#[ignore = "requires GCP KMS emulator at NEXUS_KMS_GCP_TEST_ENDPOINT"]
fn gcp_kms_unwraps_dek_against_emulator() {
    let endpoint = std::env::var("NEXUS_KMS_GCP_TEST_ENDPOINT")
        .expect("set NEXUS_KMS_GCP_TEST_ENDPOINT to run this test");
    let key_name = std::env::var("NEXUS_KMS_GCP_TEST_KEY").expect("set NEXUS_KMS_GCP_TEST_KEY");
    let dek_path = std::env::var("NEXUS_KMS_GCP_TEST_DEK").expect("set NEXUS_KMS_GCP_TEST_DEK");

    let cfg = GcpKmsConfig {
        key_name: key_name.clone(),
        wrapped_dek_path: PathBuf::from(dek_path),
        endpoint_url: Some(endpoint),
    };
    let provider = GcpKmsKeyProvider::from_config(cfg).expect("gcp kms boot");
    let key = provider.master_key().expect("master key resolution");
    assert_eq!(key.len(), MASTER_KEY_LEN);
    assert!(provider.label().contains(&key_name));
}
