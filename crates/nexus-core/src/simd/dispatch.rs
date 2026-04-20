//! Runtime CPU feature detection for SIMD dispatch.
//!
//! The probe runs exactly once per process: the first call to [`cpu()`]
//! fills a [`OnceLock<CpuFeatures>`] and every subsequent call returns
//! the same reference. Individual kernel dispatchers (see the `distance`,
//! `compare`, `reduce` submodules added in later phases) read the same
//! `CpuFeatures` value and resolve their own kernel pointer into a
//! per-operation `OnceLock`, so the cost of feature detection is
//! amortised to zero on the hot path.
//!
//! # Environment override
//!
//! When `NEXUS_SIMD_DISABLE` is set to any non-empty value the probe
//! returns a `CpuFeatures` with every flag cleared, forcing the scalar
//! fallback for every operation. This is the runtime rollback path
//! documented in ADR-003 and does not require a rebuild.

use std::sync::OnceLock;

/// Runtime-detected CPU features that SIMD kernels care about.
///
/// A flag being `true` means *both* (a) the CPU reports the feature via
/// the appropriate detection macro and (b) no compile-time `cfg` has
/// made the kernel unreachable on the current target. On non-x86_64,
/// non-aarch64 targets every flag is `false` and the scalar kernel is
/// used unconditionally.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuFeatures {
    /// x86_64 AVX-512 Foundation.
    pub avx512f: bool,
    /// x86_64 AVX-512 Vector Population Count (for bitmap popcount).
    pub avx512_vpopcntdq: bool,
    /// x86_64 AVX2 (with FMA, which every AVX2 CPU in practice has).
    pub avx2: bool,
    /// x86_64 SSE 4.2 (baseline for hardware CRC32 and string intrinsics).
    pub sse42: bool,
    /// AArch64 advanced SIMD (NEON) — always true on ARMv8.
    pub neon: bool,
    /// AArch64 SVE2 (reserved; not used in v1.0.0).
    pub sve2: bool,
    /// Process-wide override: when `true`, every dispatcher should
    /// fall back to the scalar kernel regardless of other flags.
    pub disabled: bool,
}

impl CpuFeatures {
    /// Probe the host CPU and environment. Called once per process
    /// and cached — call [`cpu()`] instead of this function directly.
    pub fn detect() -> Self {
        if std::env::var_os("NEXUS_SIMD_DISABLE")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
        {
            return Self {
                disabled: true,
                ..Self::default()
            };
        }

        #[cfg(target_arch = "x86_64")]
        {
            return Self {
                avx512f: std::is_x86_feature_detected!("avx512f"),
                avx512_vpopcntdq: std::is_x86_feature_detected!("avx512vpopcntdq"),
                avx2: std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("fma"),
                sse42: std::is_x86_feature_detected!("sse4.2"),
                neon: false,
                sve2: false,
                disabled: false,
            };
        }

        #[cfg(target_arch = "aarch64")]
        {
            return Self {
                avx512f: false,
                avx512_vpopcntdq: false,
                avx2: false,
                sse42: false,
                // NEON is a baseline feature of ARMv8; the detection
                // macro may still be used for future gating of e.g.
                // optional features like SVE2.
                neon: std::arch::is_aarch64_feature_detected!("neon"),
                sve2: std::arch::is_aarch64_feature_detected!("sve2"),
                disabled: false,
            };
        }

        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            Self::default()
        }
    }

    /// Name of the preferred kernel tier for this CPU, used in logs
    /// and in the `/stats` endpoint.
    pub fn preferred_tier(&self) -> &'static str {
        if self.disabled {
            "scalar (NEXUS_SIMD_DISABLE)"
        } else if self.avx512f {
            "avx512"
        } else if self.avx2 {
            "avx2"
        } else if self.sse42 {
            "sse4.2"
        } else if self.neon {
            "neon"
        } else {
            "scalar"
        }
    }
}

static CPU: OnceLock<CpuFeatures> = OnceLock::new();

/// Returns the process-wide cached [`CpuFeatures`].
///
/// The first call probes the host and emits a single
/// `tracing::info!` line describing the selected tier. Subsequent
/// calls are a cheap load from the `OnceLock`.
pub fn cpu() -> &'static CpuFeatures {
    CPU.get_or_init(|| {
        let features = CpuFeatures::detect();
        tracing::info!(
            tier = features.preferred_tier(),
            avx512f = features.avx512f,
            avx2 = features.avx2,
            sse42 = features.sse42,
            neon = features.neon,
            disabled = features.disabled,
            "SIMD dispatch initialised"
        );
        features
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_consistent_features_within_a_process() {
        let first = cpu();
        let second = cpu();
        assert!(std::ptr::eq(first, second));
    }

    #[test]
    fn preferred_tier_matches_highest_flag() {
        let all_off = CpuFeatures::default();
        assert_eq!(all_off.preferred_tier(), "scalar");

        let sse = CpuFeatures {
            sse42: true,
            ..CpuFeatures::default()
        };
        assert_eq!(sse.preferred_tier(), "sse4.2");

        let avx2 = CpuFeatures {
            sse42: true,
            avx2: true,
            ..CpuFeatures::default()
        };
        assert_eq!(avx2.preferred_tier(), "avx2");

        let avx512 = CpuFeatures {
            sse42: true,
            avx2: true,
            avx512f: true,
            ..CpuFeatures::default()
        };
        assert_eq!(avx512.preferred_tier(), "avx512");

        let disabled = CpuFeatures {
            avx512f: true,
            disabled: true,
            ..CpuFeatures::default()
        };
        assert_eq!(disabled.preferred_tier(), "scalar (NEXUS_SIMD_DISABLE)");
    }

    #[test]
    #[cfg(target_arch = "aarch64")]
    fn neon_is_available_on_aarch64_builds() {
        assert!(cpu().neon);
    }
}
