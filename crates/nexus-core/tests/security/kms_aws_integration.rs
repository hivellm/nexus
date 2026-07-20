//! AWS KMS integration test (localstack).
//!
//! Run manually:
//!
//! ```bash
//! docker run --rm -d --name localstack -p 4566:4566 \
//!     -e SERVICES=kms localstack/localstack:latest
//!
//! # Pre-create a KEK and wrap a 32-byte DEK:
//! aws --endpoint-url=http://localhost:4566 \
//!     --region=us-east-1 kms create-key
//!     # → take the KeyId from the response
//! aws --endpoint-url=http://localhost:4566 \
//!     --region=us-east-1 kms encrypt \
//!     --key-id <KeyId> \
//!     --plaintext "$(head -c 32 /dev/urandom | base64)" \
//!     --query CiphertextBlob --output text \
//!   | base64 -d > /tmp/wrapped-dek.bin
//!
//! NEXUS_KMS_AWS_TEST_ENDPOINT=http://localhost:4566 \
//! NEXUS_KMS_AWS_TEST_KEY_ID=<KeyId> \
//! NEXUS_KMS_AWS_TEST_DEK=/tmp/wrapped-dek.bin \
//! AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test AWS_REGION=us-east-1 \
//!     cargo test --features kms-aws --test kms_aws_integration -- --ignored --nocapture
//! ```
//!
//! Skipped by default (`#[ignore]`). The test requires a running
//! KMS endpoint and a pre-wrapped DEK; CI cannot make those
//! available without spinning up localstack, so we keep the test
//! manual until CI grows a localstack lane.

#![cfg(feature = "kms-aws")]

use std::path::PathBuf;

use nexus_core::storage::crypto::kms::{AwsKmsConfig, AwsKmsKeyProvider};
use nexus_core::storage::crypto::{KeyProvider, MASTER_KEY_LEN};

#[test]
#[ignore = "requires localstack KMS at NEXUS_KMS_AWS_TEST_ENDPOINT"]
fn aws_kms_unwraps_dek_against_localstack() {
    let endpoint = std::env::var("NEXUS_KMS_AWS_TEST_ENDPOINT")
        .expect("set NEXUS_KMS_AWS_TEST_ENDPOINT to run this test");
    let key_id = std::env::var("NEXUS_KMS_AWS_TEST_KEY_ID")
        .expect("set NEXUS_KMS_AWS_TEST_KEY_ID to run this test");
    let dek_path = std::env::var("NEXUS_KMS_AWS_TEST_DEK").expect("set NEXUS_KMS_AWS_TEST_DEK");

    let cfg = AwsKmsConfig {
        region: Some("us-east-1".to_string()),
        key_id: Some(key_id.clone()),
        wrapped_dek_path: PathBuf::from(dek_path),
        endpoint_url: Some(endpoint),
    };
    let provider = AwsKmsKeyProvider::from_config(cfg).expect("aws kms boot");
    let key = provider.master_key().expect("master key resolution");
    assert_eq!(key.len(), MASTER_KEY_LEN);
    assert!(provider.label().contains(&key_id));
}
