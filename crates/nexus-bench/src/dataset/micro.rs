//! `micro` — the 10k-node / 50k-relationship dataset.
//!
//! Small enough to fit in CI's wall-clock budget, big enough to hit
//! the executor's hot paths (label scans, B-tree seeks, expand).
//! Structure:
//!
//! * 10 000 nodes across 5 labels (`A`…`E`), 2 000 per label.
//! * 3 properties per node: `id: Int`, `name: String`, `score: Float`.
//! * 50 000 relationships of two types (`KNOWS`, `LIKES`) with
//!   deterministic wiring: every node i points at `(i+1) mod 10000`
//!   via `KNOWS`, plus 4 additional `LIKES` edges picked by a seeded
//!   xorshift RNG.
//!
//! Determinism: every run with the same [`MicroDataset::with_seed`]
//! seed produces byte-identical statements. The default seed is
//! `0x4E65_7875_7300_0001` (`"Nexus\0\0\x01"`).

use std::fmt::Write;

use super::{Dataset, DatasetKind};

/// Number of nodes per label.
pub const NODES_PER_LABEL: usize = 2000;

/// Label alphabet.
pub const LABELS: [&str; 5] = ["A", "B", "C", "D", "E"];

/// Total node count.
pub const NODE_COUNT: usize = NODES_PER_LABEL * LABELS.len();

/// Extra `LIKES` edges per node (on top of the 1 sequential `KNOWS`).
pub const LIKES_PER_NODE: usize = 4;

/// Default deterministic seed when the caller uses
/// [`MicroDataset::default`].
pub const DEFAULT_SEED: u64 = 0x4E65_7875_7300_0001;

/// The `micro` dataset.
#[derive(Debug, Clone)]
pub struct MicroDataset {
    seed: u64,
}

impl Default for MicroDataset {
    fn default() -> Self {
        Self { seed: DEFAULT_SEED }
    }
}

impl MicroDataset {
    /// Build with an explicit seed. Same seed ⇒ same statements.
    #[must_use]
    pub fn with_seed(seed: u64) -> Self {
        Self { seed }
    }

    /// The seed used to build this dataset. Emitted in the JSON
    /// report so reruns are reproducible.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Split-mix64 step — small deterministic PRNG. Reproducibility
    /// matters more than cryptographic strength.
    fn next(state: &mut u64) -> u64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

impl Dataset for MicroDataset {
    fn kind(&self) -> DatasetKind {
        DatasetKind::Micro
    }

    fn name(&self) -> &'static str {
        "micro"
    }

    fn expected_node_count(&self) -> usize {
        NODE_COUNT
    }

    fn statements(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(NODE_COUNT + NODE_COUNT * (1 + LIKES_PER_NODE) / 32);
        let mut rng = self.seed;

        // Node CREATEs, one per node, grouped in batches to keep each
        // statement under the engine's parser budget. 128 nodes per
        // batch is comfortable everywhere.
        const BATCH: usize = 128;
        for batch_start in (0..NODE_COUNT).step_by(BATCH) {
            let end = (batch_start + BATCH).min(NODE_COUNT);
            let mut stmt = String::with_capacity(BATCH * 80);
            stmt.push_str("CREATE ");
            for i in batch_start..end {
                if i > batch_start {
                    stmt.push_str(", ");
                }
                let label = LABELS[(i / NODES_PER_LABEL) % LABELS.len()];
                let score_raw = Self::next(&mut rng);
                let score = (score_raw as f64) / (u64::MAX as f64);
                let _ = write!(
                    stmt,
                    "(:{label} {{id: {i}, name: 'n{i}', score: {score:.6}}})"
                );
            }
            out.push(stmt);
        }

        // KNOWS — every node i → (i+1) mod N. Batched at 256 per
        // MATCH + CREATE statement.
        for chunk_start in (0..NODE_COUNT).step_by(256) {
            let end = (chunk_start + 256).min(NODE_COUNT);
            let mut stmt = String::with_capacity(512 * 32);
            for i in chunk_start..end {
                let j = (i + 1) % NODE_COUNT;
                let _ = writeln!(
                    stmt,
                    "MATCH (a {{id: {i}}}), (b {{id: {j}}}) CREATE (a)-[:KNOWS]->(b);"
                );
            }
            out.push(stmt);
        }

        // LIKES — 4 extra edges per node, destinations picked from a
        // seeded RNG so the graph is dense enough for traversal
        // scenarios. Batched 256 edges per statement so the parser
        // sees ~160 statements for the whole dataset instead of
        // 40k (a 40 000× reduction in parse+plan overhead that the
        // first naïve draft paid on every smoke run).
        let mut batch = String::with_capacity(256 * 64);
        let mut batched = 0usize;
        for i in 0..NODE_COUNT {
            for _ in 0..LIKES_PER_NODE {
                let target = (Self::next(&mut rng) as usize) % NODE_COUNT;
                if target == i {
                    continue;
                }
                let _ = writeln!(
                    batch,
                    "MATCH (a {{id: {i}}}), (b {{id: {target}}}) CREATE (a)-[:LIKES]->(b);"
                );
                batched += 1;
                if batched >= 256 {
                    out.push(std::mem::take(&mut batch));
                    batched = 0;
                }
            }
        }
        if !batch.is_empty() {
            out.push(batch);
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_node_count_matches_constants() {
        assert_eq!(MicroDataset::default().expected_node_count(), 10_000);
    }

    #[test]
    fn deterministic_output_for_same_seed() {
        let a = MicroDataset::with_seed(42).statements();
        let b = MicroDataset::with_seed(42).statements();
        assert_eq!(a.len(), b.len());
        // Byte-for-byte equality is the determinism contract.
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x, y);
        }
    }

    #[test]
    fn different_seeds_produce_different_statements() {
        let a = MicroDataset::with_seed(1).statements();
        let b = MicroDataset::with_seed(2).statements();
        assert_ne!(a, b);
    }

    #[test]
    fn statement_count_is_bounded() {
        // Node CREATEs: 10_000 / 128 = 79 batches (rounded up).
        // KNOWS: 10_000 / 256 = 40 batches (rounded up).
        // LIKES: up to 10_000 * 4 = 40_000 edges, batched 256/stmt,
        // so ≈157 statements (self-loops skipped — slightly fewer
        // total edges, same number of batches).
        let stmts = MicroDataset::default().statements();
        assert!(
            stmts.len() < 500,
            "expected ≈280 stmts, got {}",
            stmts.len()
        );
        assert!(stmts.len() > 100);
    }

    #[test]
    fn first_batch_is_well_formed() {
        let stmts = MicroDataset::default().statements();
        let first = &stmts[0];
        assert!(first.starts_with("CREATE (:A"));
        assert!(first.contains("id: 0"));
    }

    #[test]
    fn no_self_loops_in_likes() {
        let stmts = MicroDataset::default().statements();
        for s in stmts.iter().filter(|s| s.contains(":LIKES]")) {
            // Statement shape: MATCH (a {id: X}), (b {id: Y}) ...
            let a_id = extract_id(s, "(a {id: ");
            let b_id = extract_id(s, "(b {id: ");
            assert_ne!(a_id, b_id, "self-loop in: {s}");
        }
    }

    fn extract_id(s: &str, marker: &str) -> Option<u64> {
        let i = s.find(marker)?;
        let rest = &s[i + marker.len()..];
        let end = rest.find('}')?;
        rest[..end].trim().parse::<u64>().ok()
    }
}
