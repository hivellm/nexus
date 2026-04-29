//! Per-database key derivation via HKDF-SHA-256 (RFC 5869).
//!
//! The master key never touches a record store; every database
//! gets its own 32-byte key derived from the master via HKDF with
//! the database name as the `info` parameter. This means:
//!
//! * Compromising one database's key (a hostile DBA exfiltrates a
//!   single tenant's data) does not compromise any other database
//!   keyed off the same master.
//! * Rotating one database's key without re-encrypting the others
//!   is a one-shot HKDF call away (the rotation runner derives a
//!   new key by bumping a per-database epoch into the `info`
//!   string).
//! * The master key may live exclusively in the [`KeyProvider`]'s
//!   secure storage (KMS, env var, key file); only the derived
//!   keys travel down to the storage hooks.
//!
//! [`KeyProvider`]: super::key_provider::KeyProvider

use hkdf::Hkdf;
use sha2_010::Sha256;
use thiserror::Error;
use zeroize::Zeroizing;

use super::key_provider::MASTER_KEY_LEN;

/// Per-database key length, in bytes. Same as the master since the
/// AEAD is AES-256-GCM.
pub const DATABASE_KEY_LEN: usize = 32;

/// Domain-separation tag mixed into every HKDF expansion. Bumping
/// this constant invalidates every previously-derived key without
/// requiring a master-key rotation. Treat as a versioning lever for
/// the KDF itself.
pub const KDF_DOMAIN_TAG: &[u8] = b"nexus-encryption-at-rest-v1";

/// Errors the KDF can surface.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum KdfError {
    /// HKDF rejected the requested output length. Should never
    /// happen for [`DATABASE_KEY_LEN`] because that is well within
    /// the algorithm's `255 * HashLen` cap (8160 bytes for SHA-256),
    /// but we surface it explicitly so a future caller cannot trip
    /// it by mistake.
    #[error("ERR_KDF_BAD_LENGTH: requested {requested} bytes, max {max}")]
    BadLength { requested: usize, max: usize },
    /// The database name was empty — almost always a bug; we
    /// reject it loudly rather than silently treating it as the
    /// "default" database.
    #[error("ERR_KDF_EMPTY_DATABASE")]
    EmptyDatabase,
}

/// Wrapper around the master key. Zero-on-drop.
#[derive(Debug)]
pub struct MasterKey {
    bytes: Zeroizing<[u8; MASTER_KEY_LEN]>,
}

impl MasterKey {
    /// Wrap a raw 32-byte key.
    #[must_use]
    pub fn new(bytes: [u8; MASTER_KEY_LEN]) -> Self {
        Self {
            bytes: Zeroizing::new(bytes),
        }
    }

    /// Borrow the raw bytes for HKDF input. Internal callers only.
    pub(crate) fn as_bytes(&self) -> &[u8; MASTER_KEY_LEN] {
        &self.bytes
    }
}

/// Wrapper around a per-database key. Zero-on-drop.
#[derive(Debug)]
pub struct DatabaseKey {
    bytes: Zeroizing<[u8; DATABASE_KEY_LEN]>,
    label: String,
}

impl DatabaseKey {
    /// Borrow the raw bytes — used by the AES-GCM cipher.
    pub fn as_bytes(&self) -> &[u8; DATABASE_KEY_LEN] {
        &self.bytes
    }

    /// Label of the database this key is bound to. Used in error
    /// reports and audit logs; never as part of the keying material.
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// HKDF-SHA-256 expansion: per-database key from
/// `(master, db_name, optional epoch)`.
///
/// `epoch` is the rotation generation; pass `0` for the first
/// derivation and bump on every rotation. Different epochs produce
/// independent keys for the same database name + master pair.
pub fn derive_database_key(
    master: &MasterKey,
    db_name: &str,
    epoch: u32,
) -> Result<DatabaseKey, KdfError> {
    if db_name.is_empty() {
        return Err(KdfError::EmptyDatabase);
    }
    if DATABASE_KEY_LEN > 255 * 32 {
        return Err(KdfError::BadLength {
            requested: DATABASE_KEY_LEN,
            max: 255 * 32,
        });
    }

    // HKDF salt: stable across runs, picked so the master alone is
    // still useful for a no-salt deployment but per-database
    // derivation gets the full benefit of the salt input.
    //
    // Following RFC 5869 §3, the salt is a non-secret field; baking
    // it into a constant is a deliberate choice — operators
    // rotating data keys do so via the `epoch` field, not the
    // salt.
    const HKDF_SALT: &[u8] = b"nexus-master-key-salt-v1";

    let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), master.as_bytes());

    // The `info` string mixes the domain tag, the database name,
    // and the epoch counter. RFC 5869 §3.2 recommends `info` carry
    // every parameter that should change the derived key.
    let mut info = Vec::with_capacity(KDF_DOMAIN_TAG.len() + 1 + db_name.len() + 1 + 4);
    info.extend_from_slice(KDF_DOMAIN_TAG);
    info.push(0x1f); // unit separator
    info.extend_from_slice(db_name.as_bytes());
    info.push(0x1f);
    info.extend_from_slice(&epoch.to_be_bytes());

    let mut out = Zeroizing::new([0u8; DATABASE_KEY_LEN]);
    hk.expand(&info, &mut *out)
        .map_err(|_| KdfError::BadLength {
            requested: DATABASE_KEY_LEN,
            max: 255 * 32,
        })?;

    Ok(DatabaseKey {
        bytes: out,
        label: db_name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn master(seed: u8) -> MasterKey {
        MasterKey::new([seed; MASTER_KEY_LEN])
    }

    #[test]
    fn deterministic_for_same_inputs() {
        let m = master(1);
        let a = derive_database_key(&m, "default", 0).unwrap();
        let b = derive_database_key(&m, "default", 0).unwrap();
        assert_eq!(a.as_bytes(), b.as_bytes());
        assert_eq!(a.label(), "default");
    }

    #[test]
    fn distinct_per_database_name() {
        let m = master(1);
        let a = derive_database_key(&m, "foo", 0).unwrap();
        let b = derive_database_key(&m, "bar", 0).unwrap();
        assert_ne!(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn distinct_per_epoch() {
        let m = master(1);
        let a = derive_database_key(&m, "default", 0).unwrap();
        let b = derive_database_key(&m, "default", 1).unwrap();
        assert_ne!(a.as_bytes(), b.as_bytes(), "epoch must change the key");
    }

    #[test]
    fn distinct_per_master() {
        let a = derive_database_key(&master(1), "default", 0).unwrap();
        let b = derive_database_key(&master(2), "default", 0).unwrap();
        assert_ne!(a.as_bytes(), b.as_bytes());
    }

    #[test]
    fn empty_database_name_rejected() {
        let err = derive_database_key(&master(1), "", 0).unwrap_err();
        assert_eq!(err, KdfError::EmptyDatabase);
    }

    #[test]
    fn output_is_full_32_bytes() {
        let key = derive_database_key(&master(1), "default", 0).unwrap();
        assert_eq!(key.as_bytes().len(), DATABASE_KEY_LEN);
        // Sanity check: the derived key isn't all zeros (HKDF must
        // do work).
        assert!(key.as_bytes().iter().any(|b| *b != 0));
    }
}
