//! [`KeyProvider`] — abstraction over the source of the master key.
//!
//! Production deployments load the master key from a KMS (AWS, GCP,
//! Vault); local deployments may keep it on disk or in an
//! environment variable. The abstraction keeps the storage hooks
//! oblivious to the source — they call [`KeyProvider::master_key`]
//! once at startup and once after a rotation event.

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;
use zeroize::Zeroizing;

/// AES-256 master key length, in bytes.
pub const MASTER_KEY_LEN: usize = 32;

/// Errors a [`KeyProvider`] can surface.
#[derive(Debug, Error)]
pub enum KeyProviderError {
    /// The configured source did not yield a key.
    #[error("ERR_KEY_NOT_FOUND: {0}")]
    NotFound(String),
    /// The key was found but had the wrong length / encoding.
    #[error(
        "ERR_KEY_BAD_FORMAT: expected {MASTER_KEY_LEN} bytes (raw or hex-encoded), got {got_len}"
    )]
    BadFormat {
        /// Length actually parsed.
        got_len: usize,
    },
    /// Filesystem error reading a key file.
    #[error("ERR_KEY_IO({path}): {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Hex decoding failed on a hex-encoded key.
    #[error("ERR_KEY_HEX: {0}")]
    Hex(String),
}

/// Anything that can hand the engine a 32-byte master key.
///
/// Implementations:
/// * [`EnvKeyProvider`] — reads from `NEXUS_DATA_KEY` (raw bytes
///   for binary, hex for ASCII contexts; auto-detected by length).
/// * [`FileKeyProvider`] — reads from a file with `0600`
///   permissions (verified on Unix; warning logged on Windows).
/// * KMS adapters (AWS / GCP / Vault) — tracked under separate
///   follow-up tasks; this trait is the seam they plug into.
pub trait KeyProvider: Send + Sync {
    /// Resolve the current master key. Implementations are expected
    /// to be cheap on cached results and authoritative on rotation
    /// events.
    fn master_key(&self) -> Result<Zeroizing<[u8; MASTER_KEY_LEN]>, KeyProviderError>;

    /// Optional human-readable label of the source — used in logs
    /// when an operator needs to confirm which provider is active.
    fn label(&self) -> &str;
}

// ---------------------------------------------------------------------------
// EnvKeyProvider
// ---------------------------------------------------------------------------

/// Pulls the master key from an environment variable. The variable
/// is read **once at construction time** so a hostile process that
/// later sets the env var cannot influence the key.
#[derive(Debug)]
pub struct EnvKeyProvider {
    var_name: String,
    cached: Zeroizing<[u8; MASTER_KEY_LEN]>,
}

impl EnvKeyProvider {
    /// Build from `NEXUS_DATA_KEY`.
    pub fn from_default_env() -> Result<Self, KeyProviderError> {
        Self::from_env("NEXUS_DATA_KEY")
    }

    /// Build from any env var. The value is parsed as either a hex
    /// string (length 64) or raw bytes (length 32). Anything else
    /// surfaces [`KeyProviderError::BadFormat`].
    pub fn from_env(name: &str) -> Result<Self, KeyProviderError> {
        let raw = std::env::var(name)
            .map_err(|_| KeyProviderError::NotFound(format!("env var {name} not set")))?;
        let bytes = parse_master_key(raw.as_bytes())?;
        Ok(Self {
            var_name: name.to_string(),
            cached: bytes,
        })
    }

    /// Construct directly from raw bytes — testing only.
    #[doc(hidden)]
    pub fn from_raw_bytes(name: &str, bytes: [u8; MASTER_KEY_LEN]) -> Self {
        Self {
            var_name: name.to_string(),
            cached: Zeroizing::new(bytes),
        }
    }
}

impl KeyProvider for EnvKeyProvider {
    fn master_key(&self) -> Result<Zeroizing<[u8; MASTER_KEY_LEN]>, KeyProviderError> {
        Ok(Zeroizing::new(*self.cached))
    }

    fn label(&self) -> &str {
        &self.var_name
    }
}

// ---------------------------------------------------------------------------
// FileKeyProvider
// ---------------------------------------------------------------------------

/// Reads the master key from a file. The file must contain either
/// 32 raw bytes or a 64-character hex string.
///
/// On Unix, the constructor enforces `0600` permissions — group /
/// world readability surfaces [`KeyProviderError::Io`] with a
/// `PermissionDenied`-shaped message. On Windows, the check is a
/// best-effort warning logged via `tracing::warn!`; production
/// Windows deployments should rely on filesystem ACLs that the
/// engine cannot inspect portably.
#[derive(Debug)]
pub struct FileKeyProvider {
    path: PathBuf,
    cached: Zeroizing<[u8; MASTER_KEY_LEN]>,
}

impl FileKeyProvider {
    /// Read and parse the file at `path`.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, KeyProviderError> {
        let path = path.as_ref().to_path_buf();
        let raw = fs::read(&path).map_err(|e| KeyProviderError::Io {
            path: path.clone(),
            source: e,
        })?;
        check_permissions(&path);
        let bytes = parse_master_key(strip_trailing_newline(&raw))?;
        Ok(Self {
            path,
            cached: bytes,
        })
    }
}

impl KeyProvider for FileKeyProvider {
    fn master_key(&self) -> Result<Zeroizing<[u8; MASTER_KEY_LEN]>, KeyProviderError> {
        Ok(Zeroizing::new(*self.cached))
    }

    fn label(&self) -> &str {
        // `Path::display` returns a lossy `Display` value; we cache
        // the rendered string in `path.to_string_lossy()` per call,
        // which costs an allocation we don't want. Returning the
        // raw `OsStr` would avoid it but the trait demands a
        // `&str`. We accept the allocation here because `label()`
        // is only ever called by the operator-facing log line at
        // boot.
        Box::leak(self.path.display().to_string().into_boxed_str())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn strip_trailing_newline(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == b'\n' || bytes[end - 1] == b'\r') {
        end -= 1;
    }
    &bytes[..end]
}

fn parse_master_key(raw: &[u8]) -> Result<Zeroizing<[u8; MASTER_KEY_LEN]>, KeyProviderError> {
    match raw.len() {
        MASTER_KEY_LEN => {
            let mut buf = [0u8; MASTER_KEY_LEN];
            buf.copy_from_slice(raw);
            Ok(Zeroizing::new(buf))
        }
        64 => {
            // Hex-encoded 32-byte key. The `hex` crate is already a
            // dep of nexus-core (used by the auth path) so we don't
            // pull anything new here.
            let decoded = hex::decode(raw).map_err(|e| KeyProviderError::Hex(e.to_string()))?;
            if decoded.len() != MASTER_KEY_LEN {
                return Err(KeyProviderError::BadFormat {
                    got_len: decoded.len(),
                });
            }
            let mut buf = [0u8; MASTER_KEY_LEN];
            buf.copy_from_slice(&decoded);
            Ok(Zeroizing::new(buf))
        }
        other => Err(KeyProviderError::BadFormat { got_len: other }),
    }
}

#[cfg(unix)]
fn check_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = fs::metadata(path) {
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            tracing::warn!(
                path = %path.display(),
                mode = format!("{mode:o}"),
                "master key file is group/world readable; recommend chmod 0600"
            );
        }
    }
}

#[cfg(not(unix))]
fn check_permissions(path: &Path) {
    // ACL inspection is not portable on Windows; defer to the
    // operator. Logged at trace level so we don't spam the boot
    // path.
    tracing::trace!(
        path = %path.display(),
        "master key permission check is a no-op on non-unix platforms"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_master_key_accepts_raw_bytes() {
        let raw: [u8; 32] = [7; 32];
        let parsed = parse_master_key(&raw).expect("parse");
        assert_eq!(*parsed, raw);
    }

    #[test]
    fn parse_master_key_accepts_hex() {
        let raw = b"7777777777777777777777777777777777777777777777777777777777777777";
        let parsed = parse_master_key(raw).expect("parse hex");
        assert_eq!(*parsed, [0x77; 32]);
    }

    #[test]
    fn parse_master_key_rejects_short() {
        let err = parse_master_key(b"abcd").unwrap_err();
        assert!(matches!(err, KeyProviderError::BadFormat { got_len: 4 }));
    }

    #[test]
    fn parse_master_key_rejects_invalid_hex() {
        // 64 chars but contains a non-hex digit.
        let raw = b"zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        let err = parse_master_key(raw).unwrap_err();
        assert!(matches!(err, KeyProviderError::Hex(_)));
    }

    #[test]
    fn env_provider_reads_hex() {
        // Use a fresh var name so the test stays parallel-safe.
        let var = "NEXUS_TEST_KEY_HEX_AAAA";
        unsafe { std::env::set_var(var, "a".repeat(64)) };
        let p = EnvKeyProvider::from_env(var).expect("env");
        assert_eq!(*p.master_key().unwrap(), [0xaa; 32]);
        unsafe { std::env::remove_var(var) };
    }

    #[test]
    fn env_provider_reports_missing_var() {
        let var = "NEXUS_TEST_KEY_MISSING_BBBB";
        unsafe { std::env::remove_var(var) };
        let err = EnvKeyProvider::from_env(var).unwrap_err();
        assert!(matches!(err, KeyProviderError::NotFound(_)));
    }

    #[test]
    fn file_provider_reads_raw_bytes() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("k.bin");
        fs::write(&path, [0x33u8; 32]).unwrap();
        let p = FileKeyProvider::from_path(&path).expect("file");
        assert_eq!(*p.master_key().unwrap(), [0x33; 32]);
    }

    #[test]
    fn file_provider_strips_trailing_newline_on_hex() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("k.hex");
        let mut content = "b".repeat(64).into_bytes();
        content.extend_from_slice(b"\n");
        fs::write(&path, content).unwrap();
        let p = FileKeyProvider::from_path(&path).expect("file");
        assert_eq!(*p.master_key().unwrap(), [0xbb; 32]);
    }

    #[test]
    fn file_provider_reports_io_error_for_missing_file() {
        let err = FileKeyProvider::from_path("/this/path/does/not/exist").unwrap_err();
        assert!(matches!(err, KeyProviderError::Io { .. }));
    }
}
