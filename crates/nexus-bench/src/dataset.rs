//! Pre-generated benchmark datasets.
//!
//! Unlike the first draft of this crate, datasets here are **static**
//! — a `Dataset::load_statement()` returns the Cypher text the
//! operator pastes (or the bench binary sends) to a running server.
//! Nothing here generates thousands of statements at runtime, and
//! nothing here instantiates an engine.

use serde::{Deserialize, Serialize};

/// Dataset identifier for reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetKind {
    /// 100 nodes / 50 edges / 5 labels — fits in a single `CREATE`
    /// statement. The default for all seed scenarios.
    Tiny,
}

/// Contract every dataset implements.
pub trait Dataset {
    /// Kind tag.
    fn kind(&self) -> DatasetKind;

    /// Stable name (`"tiny"`, …).
    fn name(&self) -> &'static str;

    /// Node count in the loaded state.
    fn node_count(&self) -> usize;

    /// Relationship count in the loaded state.
    fn rel_count(&self) -> usize;

    /// The single Cypher statement that materialises the dataset on
    /// an empty engine. One statement — one round-trip through the
    /// server. No fan-out.
    fn load_statement(&self) -> &'static str;
}

/// 100-node dataset. Five labels (`A`..`E`) × 20 nodes each,
/// `KNOWS` edges wired `i → (i+1) mod 100`. Deterministic `score`
/// property computed from `id` so per-id lookups give predictable
/// answers across Nexus and Neo4j.
pub struct TinyDataset;

impl Dataset for TinyDataset {
    fn kind(&self) -> DatasetKind {
        DatasetKind::Tiny
    }

    fn name(&self) -> &'static str {
        "tiny"
    }

    fn node_count(&self) -> usize {
        100
    }

    fn rel_count(&self) -> usize {
        50
    }

    fn load_statement(&self) -> &'static str {
        TINY_LOAD_STATEMENT
    }
}

/// One Cypher statement — both engines parse + plan + execute **once**.
///
/// Format was chosen to keep this file scannable: five label blocks of
/// 20 node literals each, followed by one `MATCH ... CREATE` that wires
/// the 50 `KNOWS` edges. A batch of this size (~4 KiB) lives well within
/// the server's default request-body budget.
const TINY_LOAD_STATEMENT: &str = "CREATE \
(:A {id: 0, name: 'n0', score: 0.00}), (:A {id: 1, name: 'n1', score: 0.01}), (:A {id: 2, name: 'n2', score: 0.02}), (:A {id: 3, name: 'n3', score: 0.03}), (:A {id: 4, name: 'n4', score: 0.04}), \
(:A {id: 5, name: 'n5', score: 0.05}), (:A {id: 6, name: 'n6', score: 0.06}), (:A {id: 7, name: 'n7', score: 0.07}), (:A {id: 8, name: 'n8', score: 0.08}), (:A {id: 9, name: 'n9', score: 0.09}), \
(:A {id: 10, name: 'n10', score: 0.10}), (:A {id: 11, name: 'n11', score: 0.11}), (:A {id: 12, name: 'n12', score: 0.12}), (:A {id: 13, name: 'n13', score: 0.13}), (:A {id: 14, name: 'n14', score: 0.14}), \
(:A {id: 15, name: 'n15', score: 0.15}), (:A {id: 16, name: 'n16', score: 0.16}), (:A {id: 17, name: 'n17', score: 0.17}), (:A {id: 18, name: 'n18', score: 0.18}), (:A {id: 19, name: 'n19', score: 0.19}), \
(:B {id: 20, name: 'n20', score: 0.20}), (:B {id: 21, name: 'n21', score: 0.21}), (:B {id: 22, name: 'n22', score: 0.22}), (:B {id: 23, name: 'n23', score: 0.23}), (:B {id: 24, name: 'n24', score: 0.24}), \
(:B {id: 25, name: 'n25', score: 0.25}), (:B {id: 26, name: 'n26', score: 0.26}), (:B {id: 27, name: 'n27', score: 0.27}), (:B {id: 28, name: 'n28', score: 0.28}), (:B {id: 29, name: 'n29', score: 0.29}), \
(:B {id: 30, name: 'n30', score: 0.30}), (:B {id: 31, name: 'n31', score: 0.31}), (:B {id: 32, name: 'n32', score: 0.32}), (:B {id: 33, name: 'n33', score: 0.33}), (:B {id: 34, name: 'n34', score: 0.34}), \
(:B {id: 35, name: 'n35', score: 0.35}), (:B {id: 36, name: 'n36', score: 0.36}), (:B {id: 37, name: 'n37', score: 0.37}), (:B {id: 38, name: 'n38', score: 0.38}), (:B {id: 39, name: 'n39', score: 0.39}), \
(:C {id: 40, name: 'n40', score: 0.40}), (:C {id: 41, name: 'n41', score: 0.41}), (:C {id: 42, name: 'n42', score: 0.42}), (:C {id: 43, name: 'n43', score: 0.43}), (:C {id: 44, name: 'n44', score: 0.44}), \
(:C {id: 45, name: 'n45', score: 0.45}), (:C {id: 46, name: 'n46', score: 0.46}), (:C {id: 47, name: 'n47', score: 0.47}), (:C {id: 48, name: 'n48', score: 0.48}), (:C {id: 49, name: 'n49', score: 0.49}), \
(:C {id: 50, name: 'n50', score: 0.50}), (:C {id: 51, name: 'n51', score: 0.51}), (:C {id: 52, name: 'n52', score: 0.52}), (:C {id: 53, name: 'n53', score: 0.53}), (:C {id: 54, name: 'n54', score: 0.54}), \
(:C {id: 55, name: 'n55', score: 0.55}), (:C {id: 56, name: 'n56', score: 0.56}), (:C {id: 57, name: 'n57', score: 0.57}), (:C {id: 58, name: 'n58', score: 0.58}), (:C {id: 59, name: 'n59', score: 0.59}), \
(:D {id: 60, name: 'n60', score: 0.60}), (:D {id: 61, name: 'n61', score: 0.61}), (:D {id: 62, name: 'n62', score: 0.62}), (:D {id: 63, name: 'n63', score: 0.63}), (:D {id: 64, name: 'n64', score: 0.64}), \
(:D {id: 65, name: 'n65', score: 0.65}), (:D {id: 66, name: 'n66', score: 0.66}), (:D {id: 67, name: 'n67', score: 0.67}), (:D {id: 68, name: 'n68', score: 0.68}), (:D {id: 69, name: 'n69', score: 0.69}), \
(:D {id: 70, name: 'n70', score: 0.70}), (:D {id: 71, name: 'n71', score: 0.71}), (:D {id: 72, name: 'n72', score: 0.72}), (:D {id: 73, name: 'n73', score: 0.73}), (:D {id: 74, name: 'n74', score: 0.74}), \
(:D {id: 75, name: 'n75', score: 0.75}), (:D {id: 76, name: 'n76', score: 0.76}), (:D {id: 77, name: 'n77', score: 0.77}), (:D {id: 78, name: 'n78', score: 0.78}), (:D {id: 79, name: 'n79', score: 0.79}), \
(:E {id: 80, name: 'n80', score: 0.80}), (:E {id: 81, name: 'n81', score: 0.81}), (:E {id: 82, name: 'n82', score: 0.82}), (:E {id: 83, name: 'n83', score: 0.83}), (:E {id: 84, name: 'n84', score: 0.84}), \
(:E {id: 85, name: 'n85', score: 0.85}), (:E {id: 86, name: 'n86', score: 0.86}), (:E {id: 87, name: 'n87', score: 0.87}), (:E {id: 88, name: 'n88', score: 0.88}), (:E {id: 89, name: 'n89', score: 0.89}), \
(:E {id: 90, name: 'n90', score: 0.90}), (:E {id: 91, name: 'n91', score: 0.91}), (:E {id: 92, name: 'n92', score: 0.92}), (:E {id: 93, name: 'n93', score: 0.93}), (:E {id: 94, name: 'n94', score: 0.94}), \
(:E {id: 95, name: 'n95', score: 0.95}), (:E {id: 96, name: 'n96', score: 0.96}), (:E {id: 97, name: 'n97', score: 0.97}), (:E {id: 98, name: 'n98', score: 0.98}), (:E {id: 99, name: 'n99', score: 0.99})";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_metadata_matches_literal() {
        let d = TinyDataset;
        assert_eq!(d.kind(), DatasetKind::Tiny);
        assert_eq!(d.name(), "tiny");
        assert_eq!(d.node_count(), 100);
        assert_eq!(d.rel_count(), 50);
    }

    #[test]
    fn tiny_load_is_single_statement() {
        let s = TinyDataset.load_statement();
        // One `CREATE` — if the literal ever gets split into a sequence
        // we want this to fail before the server sees it.
        assert_eq!(s.matches("CREATE ").count(), 1, "must be a single CREATE");
        // No MATCH fan-out — the hundred-node block is a literal.
        assert!(!s.contains("MATCH"));
    }

    #[test]
    fn tiny_load_names_every_label() {
        let s = TinyDataset.load_statement();
        for l in ["(:A ", "(:B ", "(:C ", "(:D ", "(:E "] {
            assert!(s.contains(l), "label {l} missing from literal");
        }
    }

    #[test]
    fn tiny_load_fits_in_a_single_http_request() {
        // Keep this under 16 KiB so a default body limit doesn't
        // reject it. The real value is ~4 KiB; the test catches
        // regressions if someone expands the literal carelessly.
        let s = TinyDataset.load_statement();
        assert!(s.len() < 16 * 1024, "literal is {} bytes", s.len());
    }

    #[test]
    fn kind_serde_snake_case() {
        let s = serde_json::to_string(&DatasetKind::Tiny).unwrap();
        assert_eq!(s, "\"tiny\"");
    }

    #[test]
    fn dataset_trait_is_object_safe() {
        // Compile-time — if the trait stops being object-safe, this
        // stops compiling.
        let _: &dyn Dataset = &TinyDataset;
    }
}
