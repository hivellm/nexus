//! Password hashing and verification using SHA512

use sha2::{Digest, Sha512};

/// Hash a password using SHA512
pub fn hash_password(password: &str) -> String {
    let mut hasher = Sha512::new();
    hasher.update(password.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Verify a password against a hash
pub fn verify_password(password: &str, hash: &str) -> bool {
    let computed_hash = hash_password(password);
    computed_hash == hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hashing() {
        let password = "test_password_123";
        let hash1 = hash_password(password);
        let hash2 = hash_password(password);

        // Same password should produce same hash
        assert_eq!(hash1, hash2);

        // Hash should be 128 characters (64 bytes * 2 for hex)
        assert_eq!(hash1.len(), 128);
    }

    #[test]
    fn test_password_verification() {
        let password = "test_password_123";
        let hash = hash_password(password);

        assert!(verify_password(password, &hash));
        assert!(!verify_password("wrong_password", &hash));
    }

    #[test]
    fn test_different_passwords_produce_different_hashes() {
        let hash1 = hash_password("password1");
        let hash2 = hash_password("password2");

        assert_ne!(hash1, hash2);
    }
}
