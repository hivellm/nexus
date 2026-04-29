//! AES-256-GCM page encryption with deterministic per-page nonces.
//!
//! # Nonce derivation
//!
//! AES-GCM is **catastrophically broken** under nonce reuse with
//! the same key — an adversary that observes two ciphertexts under
//! the same `(key, nonce)` can recover the authentication key. We
//! commit to never reusing a `(database_key, nonce)` pair across
//! writes through three derivation inputs baked into the 96-bit
//! nonce:
//!
//! 1. `file_id` (16 bits) — uniquely identifies the on-disk file
//!    within the database. Catalog, record stores, WAL, each index
//!    file get their own id.
//! 2. `page_offset` (48 bits) — byte offset of the page within the
//!    file. Aligned to the page size, so the low bits are zero;
//!    we still consume 48 bits because rotation has 4 KiB pages
//!    on terabyte files (`4 KiB × 2^36 ≈ 256 TiB` addressable).
//! 3. `page_generation` (32 bits) — bumps on every overwrite of
//!    the same page. Initialised to `1` for fresh pages and
//!    monotonically incremented by the storage hook on every
//!    write. This is the field that prevents nonce reuse on
//!    in-place page updates.
//!
//! Total: 16 + 48 + 32 = 96 bits = the AES-GCM nonce length.
//!
//! The contract is documented exhaustively because nonce reuse is
//! the #1 way to break AES-GCM in production; storage hooks must
//! never construct a [`PageNonce`] without bumping the generation
//! counter.

use std::convert::TryInto;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use thiserror::Error;

use super::kdf::DatabaseKey;

/// AES-GCM nonce length in bytes (96 bits — required by the spec).
pub const NONCE_LEN: usize = 12;

/// AES-GCM authentication tag length in bytes (128 bits — the
/// standard, and the only length the `aes-gcm` crate exposes via
/// the high-level `Aead` trait).
pub const TAG_LEN: usize = 16;

/// Errors the AEAD can surface.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AeadError {
    /// Decryption failed — either the ciphertext was tampered with,
    /// the wrong key is in use, or the nonce was reused. The error
    /// is intentionally vague so an attacker probing the surface
    /// cannot distinguish those three cases (a CCA-2 oracle would
    /// otherwise leak information).
    #[error(
        "ERR_BAD_KEY: AEAD decryption failed (tampered ciphertext, wrong key, or nonce mismatch)"
    )]
    BadKey,
    /// Caller passed an empty plaintext / ciphertext.
    #[error("ERR_AEAD_EMPTY")]
    Empty,
}

/// Per-page nonce material. Construct via [`PageNonce::new`]; the
/// storage hooks own the bookkeeping that guarantees `(file_id,
/// page_offset, generation)` uniqueness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageNonce {
    pub file_id: u16,
    /// Byte offset of the page within the file. Stored as `u64`
    /// for ergonomics; truncated to 48 bits when packed.
    pub page_offset: u64,
    pub generation: u32,
}

impl PageNonce {
    /// Build from raw fields. The caller is responsible for
    /// generation monotonicity.
    #[must_use]
    pub fn new(file_id: u16, page_offset: u64, generation: u32) -> Self {
        Self {
            file_id,
            page_offset,
            generation,
        }
    }

    /// Pack into the 12-byte AES-GCM nonce.
    ///
    /// Layout (big-endian, network order):
    ///
    /// ```text
    ///   bytes [0..2]   = file_id
    ///   bytes [2..8]   = page_offset >> 0 (low 48 bits)
    ///   bytes [8..12]  = generation
    /// ```
    pub fn to_bytes(self) -> [u8; NONCE_LEN] {
        let mut buf = [0u8; NONCE_LEN];
        buf[0..2].copy_from_slice(&self.file_id.to_be_bytes());
        // Mask down to the low 48 bits then big-endian pack.
        let off48 = self.page_offset & 0x0000_FFFF_FFFF_FFFF;
        let off_be = off48.to_be_bytes(); // 8 bytes
        buf[2..8].copy_from_slice(&off_be[2..8]); // drop top 2 bytes
        buf[8..12].copy_from_slice(&self.generation.to_be_bytes());
        buf
    }

    /// Inverse of [`Self::to_bytes`]. Used by tests and debugging
    /// tools; production never round-trips a nonce because the
    /// fields are always reconstructible from the page header.
    #[doc(hidden)]
    #[must_use]
    pub fn from_bytes(buf: [u8; NONCE_LEN]) -> Self {
        let file_id = u16::from_be_bytes(buf[0..2].try_into().expect("len=2"));
        let mut off_be = [0u8; 8];
        off_be[2..8].copy_from_slice(&buf[2..8]);
        let page_offset = u64::from_be_bytes(off_be);
        let generation = u32::from_be_bytes(buf[8..12].try_into().expect("len=4"));
        Self {
            file_id,
            page_offset,
            generation,
        }
    }
}

/// AES-256-GCM cipher bound to a database key. Owns the inner
/// `Aes256Gcm` so the hot path only pays one HMAC + one CTR pass
/// per page.
pub struct PageCipher {
    inner: Aes256Gcm,
}

impl PageCipher {
    /// Bind a cipher to the given per-database key.
    #[must_use]
    pub fn new(key: &DatabaseKey) -> Self {
        let k = Key::<Aes256Gcm>::from_slice(key.as_bytes());
        Self {
            inner: Aes256Gcm::new(k),
        }
    }
}

/// Encrypt a page payload. Returns ciphertext **with** the 16-byte
/// AES-GCM tag appended (the `aes-gcm` crate's `Aead` trait emits
/// `ciphertext || tag` by convention).
///
/// The optional `aad` (additional authenticated data) is bound into
/// the tag without being encrypted. Storage hooks may pass page
/// metadata (file id, offset, generation) so a swapped header is
/// caught at decrypt time.
pub fn encrypt_page(
    cipher: &PageCipher,
    nonce: PageNonce,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, AeadError> {
    if plaintext.is_empty() {
        return Err(AeadError::Empty);
    }
    let nonce_bytes = nonce.to_bytes();
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .inner
        .encrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| AeadError::BadKey)
}

/// Decrypt and authenticate a ciphertext produced by
/// [`encrypt_page`]. The `aad` MUST match the value used at
/// encryption — a mismatch surfaces [`AeadError::BadKey`].
pub fn decrypt_page(
    cipher: &PageCipher,
    nonce: PageNonce,
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, AeadError> {
    if ciphertext.is_empty() {
        return Err(AeadError::Empty);
    }
    let nonce_bytes = nonce.to_bytes();
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .inner
        .decrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| AeadError::BadKey)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::crypto::kdf::{MasterKey, derive_database_key};

    fn fresh_key(seed: u8, db: &str) -> DatabaseKey {
        let m = MasterKey::new([seed; 32]);
        derive_database_key(&m, db, 0).unwrap()
    }

    #[test]
    fn nonce_layout_is_stable() {
        let n = PageNonce::new(0xCAFE, 0x0011_2233_4455, 0x7788_99AA);
        let bytes = n.to_bytes();
        assert_eq!(bytes[0..2], [0xCA, 0xFE]);
        assert_eq!(bytes[2..8], [0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
        assert_eq!(bytes[8..12], [0x77, 0x88, 0x99, 0xAA]);
        // Round-trip.
        let parsed = PageNonce::from_bytes(bytes);
        assert_eq!(parsed, n);
    }

    #[test]
    fn nonce_truncates_offset_above_48_bits() {
        // The top 16 bits of `page_offset` are dropped at pack time
        // — this is intentional but worth a test so a future
        // refactor doesn't silently change it.
        let n = PageNonce::new(1, 0xFFFF_0000_0000_0001, 1);
        let bytes = n.to_bytes();
        assert_eq!(bytes[2..8], [0x00, 0x00, 0x00, 0x00, 0x00, 0x01]);
    }

    #[test]
    fn round_trip_recovers_plaintext() {
        let key = fresh_key(7, "default");
        let cipher = PageCipher::new(&key);
        let plaintext = b"node-record-bytes-here";
        let aad = b"file=catalog;offset=0;gen=1";
        let nonce = PageNonce::new(1, 0, 1);
        let ct = encrypt_page(&cipher, nonce, plaintext, aad).expect("encrypt");
        // Tag is appended -> ciphertext.len() == plaintext.len() + TAG_LEN.
        assert_eq!(ct.len(), plaintext.len() + TAG_LEN);
        let pt = decrypt_page(&cipher, nonce, &ct, aad).expect("decrypt");
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn wrong_key_fails_with_bad_key_error() {
        let cipher_a = PageCipher::new(&fresh_key(1, "default"));
        let cipher_b = PageCipher::new(&fresh_key(2, "default"));
        let nonce = PageNonce::new(1, 0, 1);
        let ct = encrypt_page(&cipher_a, nonce, b"secret", b"").unwrap();
        let err = decrypt_page(&cipher_b, nonce, &ct, b"").unwrap_err();
        assert_eq!(err, AeadError::BadKey);
    }

    #[test]
    fn wrong_database_key_fails() {
        let cipher_foo = PageCipher::new(&fresh_key(1, "foo"));
        let cipher_bar = PageCipher::new(&fresh_key(1, "bar"));
        let nonce = PageNonce::new(1, 0, 1);
        let ct = encrypt_page(&cipher_foo, nonce, b"secret", b"").unwrap();
        let err = decrypt_page(&cipher_bar, nonce, &ct, b"").unwrap_err();
        assert_eq!(err, AeadError::BadKey);
    }

    #[test]
    fn aad_mismatch_fails() {
        let cipher = PageCipher::new(&fresh_key(1, "default"));
        let nonce = PageNonce::new(1, 0, 1);
        let ct = encrypt_page(&cipher, nonce, b"secret", b"aad-1").unwrap();
        let err = decrypt_page(&cipher, nonce, &ct, b"aad-2").unwrap_err();
        assert_eq!(err, AeadError::BadKey);
    }

    #[test]
    fn nonce_mismatch_fails() {
        let cipher = PageCipher::new(&fresh_key(1, "default"));
        let nonce_a = PageNonce::new(1, 0, 1);
        let nonce_b = PageNonce::new(1, 0, 2);
        let ct = encrypt_page(&cipher, nonce_a, b"secret", b"").unwrap();
        let err = decrypt_page(&cipher, nonce_b, &ct, b"").unwrap_err();
        assert_eq!(err, AeadError::BadKey);
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let cipher = PageCipher::new(&fresh_key(1, "default"));
        let nonce = PageNonce::new(1, 0, 1);
        let mut ct = encrypt_page(&cipher, nonce, b"secret", b"").unwrap();
        ct[0] ^= 0x01; // flip one bit
        let err = decrypt_page(&cipher, nonce, &ct, b"").unwrap_err();
        assert_eq!(err, AeadError::BadKey);
    }

    #[test]
    fn empty_input_rejected_explicitly() {
        let cipher = PageCipher::new(&fresh_key(1, "default"));
        let nonce = PageNonce::new(1, 0, 1);
        assert_eq!(
            encrypt_page(&cipher, nonce, b"", b"").unwrap_err(),
            AeadError::Empty
        );
        assert_eq!(
            decrypt_page(&cipher, nonce, b"", b"").unwrap_err(),
            AeadError::Empty
        );
    }

    #[test]
    fn distinct_nonces_produce_distinct_ciphertexts() {
        let cipher = PageCipher::new(&fresh_key(1, "default"));
        let nonce_a = PageNonce::new(1, 0, 1);
        let nonce_b = PageNonce::new(1, 0, 2);
        let ct_a = encrypt_page(&cipher, nonce_a, b"identical", b"").unwrap();
        let ct_b = encrypt_page(&cipher, nonce_b, b"identical", b"").unwrap();
        assert_ne!(ct_a, ct_b, "different nonces must produce different ct");
    }

    #[test]
    fn ciphertext_does_not_leak_plaintext_pattern() {
        // Repeating plaintext of all-zeroes; the ciphertext must not
        // be all-zeroes (a stream cipher property AES-GCM has by
        // construction). Catches a future regression where a
        // hypothetical refactor accidentally bypasses the cipher.
        let cipher = PageCipher::new(&fresh_key(1, "default"));
        let nonce = PageNonce::new(1, 0, 1);
        let plaintext = vec![0u8; 4096];
        let ct = encrypt_page(&cipher, nonce, &plaintext, b"").unwrap();
        assert!(ct.iter().any(|b| *b != 0));
    }
}
