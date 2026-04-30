//! Managed-KMS [`KeyProvider`] adapters.
//!
//! # Why
//!
//! `EnvKeyProvider` and `FileKeyProvider` are fine for self-hosted
//! evaluation deployments — operators paste the master key into an
//! env var or chmod 0600 a key file and ship it. Production cloud
//! deployments need to delegate master-key custody to a managed
//! KMS: AWS KMS, GCP KMS, or HashiCorp Vault. The KMS holds the
//! **key encryption key** (KEK); we only ever see the unwrapped
//! 32-byte data key (the **DEK**), and only in process memory,
//! wrapped in [`zeroize::Zeroizing`] so it gets wiped on drop.
//!
//! # DEK pattern
//!
//! Each adapter holds two pieces of state:
//!
//! 1. A reference to a KMS-owned KEK (an ARN, a resource path, a
//!    transit-engine key name).
//! 2. A blob of **wrapped DEK** ciphertext that the operator
//!    generated once via the KMS' `Encrypt` / `wrap` call and
//!    stashed somewhere the engine can read at boot (env var,
//!    file on disk, etc.).
//!
//! At construction time the adapter calls the KMS once to unwrap
//! the DEK, caches the 32-byte plaintext for the process lifetime,
//! and returns it from [`KeyProvider::master_key`] thereafter. The
//! KMS is never on the hot path — a transient outage after boot
//! does not affect serving traffic.
//!
//! # Errors
//!
//! Every adapter surfaces transient and permanent failures through
//! the shared [`KmsError`] type. The boot path bubbles `KmsError`
//! up as `KeyProviderError::NotFound` with a structured cause
//! string, so operators see the same `ERR_KEY_*` taxonomy across
//! every provider. See `docs/security/ENCRYPTION_AT_REST.md`
//! § "KMS integration" for the operator-facing error catalogue.
//!
//! # Feature gating
//!
//! Each provider lives behind its own Cargo feature so the
//! default build — what the dev / CI matrix exercises — does not
//! pay the SDK transitive-dep cost. Enable with `--features
//! kms-aws|kms-gcp|kms-vault`, or `--features kms` for all three.

use thiserror::Error;
use zeroize::Zeroizing;

use super::key_provider::{KeyProvider, KeyProviderError, MASTER_KEY_LEN};

#[cfg(feature = "kms-aws")]
pub mod aws;
#[cfg(feature = "kms-gcp")]
pub mod gcp;
#[cfg(feature = "kms-vault")]
pub mod vault;

#[cfg(feature = "kms-aws")]
pub use aws::{AwsKmsConfig, AwsKmsKeyProvider};
#[cfg(feature = "kms-gcp")]
pub use gcp::{GcpKmsConfig, GcpKmsKeyProvider};
#[cfg(feature = "kms-vault")]
pub use vault::{VaultConfig, VaultKeyProvider};

/// Structured failures from a KMS adapter. Variants are coarse on
/// purpose — every concrete provider maps its richer error type
/// into one of these so the operator-facing log line stays uniform
/// regardless of which KMS is configured.
#[derive(Debug, Error)]
pub enum KmsError {
    /// Adapter could not be constructed because operator-supplied
    /// configuration was missing or malformed (no KEK arn, no
    /// wrapped-DEK file, etc.).
    #[error("ERR_KEY_KMS_CONFIG: {0}")]
    Config(String),
    /// The KMS request itself failed — network error, auth
    /// rejection, throttling, KEK not found, region mismatch. The
    /// inner message preserves the SDK's diagnostic for the
    /// operator log.
    #[error("ERR_KEY_KMS_FAILURE: {0}")]
    Request(String),
    /// The KMS returned a payload whose plaintext was not exactly
    /// 32 bytes long. Either the wrapped blob was generated for a
    /// different scheme or it was corrupted in transit / at rest.
    #[error("ERR_KEY_KMS_BAD_LENGTH: expected {MASTER_KEY_LEN} plaintext bytes, got {got_len}")]
    BadLength {
        /// Length actually returned by the KMS.
        got_len: usize,
    },
}

impl From<KmsError> for KeyProviderError {
    fn from(e: KmsError) -> Self {
        // Surface every KMS failure through the existing `NotFound`
        // taxonomy — the boot path already treats `NotFound` as a
        // hard fail (see `resolve_encryption_config`), and a KMS
        // outage at boot is operationally indistinguishable from
        // "the key is not where you said it is".
        KeyProviderError::NotFound(e.to_string())
    }
}

/// Convert a 32-byte KMS plaintext into the trait-layer key shape.
///
/// Used by every adapter to validate length once before caching.
pub(crate) fn wrap_master_key(
    plaintext: &[u8],
) -> Result<Zeroizing<[u8; MASTER_KEY_LEN]>, KmsError> {
    if plaintext.len() != MASTER_KEY_LEN {
        return Err(KmsError::BadLength {
            got_len: plaintext.len(),
        });
    }
    let mut buf = [0u8; MASTER_KEY_LEN];
    buf.copy_from_slice(plaintext);
    Ok(Zeroizing::new(buf))
}

/// Run a future to completion from a synchronous context.
///
/// `KeyProvider::master_key` is sync but every cloud-KMS SDK is
/// async, so each adapter resolves the master key once at
/// construction time on whatever runtime is current. If a tokio
/// runtime is already active (the normal case — server boot runs
/// inside `#[tokio::main]`), we hand the future to a fresh
/// blocking thread to avoid `Handle::block_on` deadlocking on the
/// caller's worker. Otherwise we spin up a single-thread runtime
/// for the unwrap call only.
#[cfg(any(feature = "kms-aws", feature = "kms-gcp", feature = "kms-vault"))]
pub(crate) fn block_on<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => std::thread::scope(|s| {
            s.spawn(|| handle.block_on(fut)).join().unwrap_or_else(|e| {
                std::panic::resume_unwind(e);
            })
        }),
        Err(_) => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("constructing a current-thread tokio runtime cannot fail on supported platforms");
            rt.block_on(fut)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_master_key_accepts_exact_length() {
        let raw = [0xAB; MASTER_KEY_LEN];
        let wrapped = wrap_master_key(&raw).expect("32 bytes is the contract");
        assert_eq!(*wrapped, raw);
    }

    #[test]
    fn wrap_master_key_rejects_short() {
        let err = wrap_master_key(&[0u8; 16]).unwrap_err();
        assert!(matches!(err, KmsError::BadLength { got_len: 16 }));
    }

    #[test]
    fn wrap_master_key_rejects_long() {
        let err = wrap_master_key(&[0u8; 64]).unwrap_err();
        assert!(matches!(err, KmsError::BadLength { got_len: 64 }));
    }

    #[test]
    fn kms_error_maps_to_key_provider_not_found() {
        let kms_err = KmsError::Request("synthetic outage".into());
        let provider_err: KeyProviderError = kms_err.into();
        assert!(
            matches!(provider_err, KeyProviderError::NotFound(s) if s.contains("synthetic outage"))
        );
    }
}

// Compile-time witness that the KeyProvider trait stays object-safe
// after the KMS adapters land. Removing this `_` lets the providers
// drift into trait shapes that boot wiring cannot store behind
// `Box<dyn KeyProvider>`, which is the contract the server config
// already commits to.
#[cfg(any(feature = "kms-aws", feature = "kms-gcp", feature = "kms-vault"))]
const _: fn() = || {
    let _: Option<Box<dyn KeyProvider>> = None;
};
