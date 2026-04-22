//! Hash-based shard assignment.
//!
//! The sharding function is `shard_id = xxh3(node_id_le_bytes) mod num_shards`.
//! xxh3 is the same fast non-cryptographic hash Nexus already uses elsewhere
//! (`xxhash-rust` workspace dep, SIMD-accelerated on x86_64 / aarch64), so the
//! cost of assignment is well under 10ns per call on a modern core.
//!
//! # Why xxh3 and not CRC32
//!
//! CRC32 is fast but skews when node ids are sequential — a monotonically
//! assigned id generator (how Nexus allocates node ids) produces a strongly
//! non-uniform CRC32 distribution when taken modulo small shard counts.
//! xxh3's avalanche property gives ±15% balance across 8 shards over 10k
//! sequential ids (exercised in [`tests::sequential_ids_balance_within_15pct`]).

use super::metadata::ShardId;

/// Internal-form node id (u64) used by the storage layer.
pub type StorageNodeId = u64;

/// Compute the shard that owns a given `node_id`.
///
/// Panics only if `num_shards == 0`, which is a programmer error: every
/// caller above this has already validated that the cluster has at least one
/// shard (via [`super::metadata::ClusterMeta::validate`]).
#[inline]
#[must_use]
pub fn assign_shard(node_id: StorageNodeId, num_shards: u32) -> ShardId {
    assert!(
        num_shards > 0,
        "assign_shard called with num_shards == 0; violates ClusterMeta invariant"
    );
    let h = xxhash_rust::xxh3::xxh3_64(&node_id.to_le_bytes());
    // `num_shards` is u32 so the cast is always safe.
    ShardId::new((h % u64::from(num_shards)) as u32)
}

/// Convenience alias — same as [`assign_shard`] but taking the node id as a
/// `u64` reference. Matches the spelling used elsewhere in the executor.
#[inline]
#[must_use]
pub fn shard_for_node_u64(node_id: &StorageNodeId, num_shards: u32) -> ShardId {
    assign_shard(*node_id, num_shards)
}

/// Convenience form taking any `Into<StorageNodeId>`. The executor has
/// several id types (`u32`, `u64`, a newtype wrapper) that all reduce to a
/// `u64` at the storage boundary, so this keeps call sites tidy.
#[inline]
#[must_use]
pub fn shard_for_node<N: Into<StorageNodeId>>(node_id: N, num_shards: u32) -> ShardId {
    assign_shard(node_id.into(), num_shards)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_across_calls() {
        for id in [0u64, 1, 42, 1 << 32, u64::MAX] {
            let a = assign_shard(id, 8);
            let b = assign_shard(id, 8);
            assert_eq!(a, b, "assignment must be deterministic for node_id={id}");
        }
    }

    #[test]
    fn respects_num_shards_range() {
        for id in 0u64..1_000 {
            let s = assign_shard(id, 4);
            assert!(s.as_u32() < 4, "shard {s:?} out of range for num_shards=4");
        }
    }

    #[test]
    #[should_panic(expected = "num_shards == 0")]
    fn zero_shards_panics() {
        let _ = assign_shard(42, 0);
    }

    #[test]
    fn single_shard_always_zero() {
        for id in 0u64..100 {
            assert_eq!(assign_shard(id, 1), ShardId::new(0));
        }
    }

    #[test]
    fn sequential_ids_balance_within_15pct() {
        const NUM_SHARDS: u32 = 8;
        const N: u64 = 10_000;
        let mut counts = vec![0u64; NUM_SHARDS as usize];
        for id in 0..N {
            let s = assign_shard(id, NUM_SHARDS).as_u32() as usize;
            counts[s] += 1;
        }
        let mean = N / u64::from(NUM_SHARDS);
        let tol = mean * 15 / 100; // ±15%
        for (i, &c) in counts.iter().enumerate() {
            let diff = c.abs_diff(mean);
            assert!(
                diff <= tol,
                "shard {i} has {c} ids (mean={mean}, tol=±{tol}) — xxh3 distribution degraded"
            );
        }
    }

    #[test]
    fn all_u32_edge_values_hash_in_range() {
        // Exercises low-entropy inputs that broke CRC32-based schemes.
        for id in [0u64, 1, 2, 0xFFFF_FFFF, 0xFFFF_FFFF_FFFF_FFFF] {
            let s = assign_shard(id, 16);
            assert!(s.as_u32() < 16, "boundary id {id} produced shard {s:?}");
        }
    }

    #[test]
    fn u32_into_form_matches_raw_form() {
        let id: u32 = 12345;
        assert_eq!(
            shard_for_node(id, 8),
            assign_shard(u64::from(id), 8),
            "convenience form must agree with raw form"
        );
    }

    #[test]
    fn reference_form_matches_value_form() {
        let id: StorageNodeId = 999;
        assert_eq!(shard_for_node_u64(&id, 7), assign_shard(id, 7));
    }
}
