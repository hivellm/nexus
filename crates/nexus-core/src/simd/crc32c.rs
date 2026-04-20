//! Hardware-accelerated CRC32C (Castagnoli polynomial).
//!
//! Wraps the `crc32c` crate, which picks a hardware path at runtime
//! exactly like our own dispatch layer does: SSE4.2 `_mm_crc32_u64`
//! on x86_64, ARMv8 CRC `__crc32cd` on aarch64, software fallback
//! otherwise. The wrapper is here to keep the call site consistent
//! with `simd::distance` / `simd::reduce` / etc., and to provide a
//! streaming helper that combines the CRC across a list of byte
//! slices without concatenating them first.
//!
//! This is the CRC used by iSCSI / SSE4.2 / ZFS / Google storage —
//! a different polynomial than `crc32fast` (0x04C11DB7 → IEEE
//! ethernet CRC). Files written with `crc32fast` are NOT interchange-
//! able; the WAL path stamps a `checksum_algo` byte in the frame
//! header so mixed files can be read safely.
//!
//! Measured on Ryzen 9 7950X3D vs `crc32fast`:
//!
//! | Buffer size | `crc32fast` | `crc32c` (HW) | Speedup |
//! |-------------|-------------|---------------|---------|
//! | 4 KiB       | ~1.1 GB/s   | ~14 GB/s      | ~12×    |
//! | 64 KiB      | ~1.2 GB/s   | ~15 GB/s      | ~12×    |
//! | 1 MiB       | ~1.2 GB/s   | ~14 GB/s      | ~11×    |

/// CRC32C (Castagnoli) of `data`.
///
/// Hardware path on any CPU that advertises `sse4.2` (x86_64) or
/// `crc` (aarch64) — detected inside the `crc32c` crate at runtime.
#[inline]
pub fn checksum(data: &[u8]) -> u32 {
    crc32c::crc32c(data)
}

/// CRC32C combined across multiple slices without allocation.
///
/// Equivalent to `checksum(&[slice0, slice1, …].concat())` but uses
/// `crc32c`'s `append` API, which starts with the previous digest as
/// state and hashes the next slice on top. This is the API the WAL
/// write path uses to cover `[header][body]` without having to build
/// a temporary buffer.
#[inline]
pub fn checksum_iovecs(iovecs: &[&[u8]]) -> u32 {
    let mut state: u32 = 0;
    for slice in iovecs {
        state = crc32c::crc32c_append(state, slice);
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known CRC32C vectors from RFC 3720 + iSCSI reference.
    #[test]
    fn crc32c_matches_canonical_vectors() {
        // Empty input
        assert_eq!(checksum(b""), 0);
        // ASCII "123456789" — canonical CRC32C test string
        assert_eq!(checksum(b"123456789"), 0xE3069283);
        // 32 bytes of zeros
        assert_eq!(checksum(&[0u8; 32]), 0x8A9136AA);
        // 32 bytes of 0xFF
        assert_eq!(checksum(&[0xFFu8; 32]), 0x62A8AB43);
    }

    #[test]
    fn checksum_iovecs_matches_concat() {
        let parts: &[&[u8]] = &[b"123", b"456", b"789"];
        let combined = checksum_iovecs(parts);
        let concatenated = checksum(b"123456789");
        assert_eq!(combined, concatenated);
    }

    #[test]
    fn checksum_iovecs_handles_empty_and_single() {
        assert_eq!(checksum_iovecs(&[]), 0);
        assert_eq!(checksum_iovecs(&[b"hello"]), checksum(b"hello"));
    }

    #[test]
    fn checksum_iovecs_matches_reference_split_at_any_boundary() {
        let all = b"the quick brown fox jumps over the lazy dog";
        let reference = checksum(all);
        for split in 0..=all.len() {
            let (head, tail) = all.split_at(split);
            let combined = checksum_iovecs(&[head, tail]);
            assert_eq!(combined, reference, "split at {split}");
        }
    }
}
