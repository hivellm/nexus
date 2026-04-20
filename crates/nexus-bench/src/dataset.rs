//! Pre-generated benchmark datasets.
//!
//! Unlike the first draft of this crate, datasets here are **static**
//! — a `Dataset::load_statement()` returns the Cypher text the
//! operator pastes (or the bench binary sends) to a running server.
//! Nothing here generates thousands of statements at runtime, and
//! nothing here instantiates an engine.

use serde::{Deserialize, Serialize};

/// Dataset identifier for reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatasetKind {
    /// 100 nodes / 50 edges / 5 labels — fits in a single `CREATE`
    /// statement. The default for most seed scenarios.
    Tiny,
    /// 50 nodes / 53 edges / 1 label — a hub-plus-chain topology
    /// that supports non-trivial 1-hop / 2-hop / variable-length
    /// traversal scenarios. Still a single `CREATE`, still ~2 KiB
    /// literal, still no fan-out.
    Small,
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

/// 100-node dataset. Five labels (`A`..`E`) × 20 nodes each, and a
/// 50-edge `KNOWS` chain `(n0)→(n1)→…→(n50)` wired through the
/// first 51 nodes. Deterministic `score` property computed from
/// `id` so per-id lookups give predictable answers across Nexus
/// and Neo4j.
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
/// 20 node literals each (variables `n0`..`n99` bound so the edge
/// section can reference them in the same statement), then five rows
/// of 10 `KNOWS` edges forming a 50-edge chain through the first 51
/// nodes. Everything stays inside one `CREATE` — no `MATCH`, no
/// second round-trip. The batch is ~6 KiB, well under the server's
/// default request-body budget.
const TINY_LOAD_STATEMENT: &str = "CREATE \
(n0:A {id: 0, name: 'n0', score: 0.00}), (n1:A {id: 1, name: 'n1', score: 0.01}), (n2:A {id: 2, name: 'n2', score: 0.02}), (n3:A {id: 3, name: 'n3', score: 0.03}), (n4:A {id: 4, name: 'n4', score: 0.04}), \
(n5:A {id: 5, name: 'n5', score: 0.05}), (n6:A {id: 6, name: 'n6', score: 0.06}), (n7:A {id: 7, name: 'n7', score: 0.07}), (n8:A {id: 8, name: 'n8', score: 0.08}), (n9:A {id: 9, name: 'n9', score: 0.09}), \
(n10:A {id: 10, name: 'n10', score: 0.10}), (n11:A {id: 11, name: 'n11', score: 0.11}), (n12:A {id: 12, name: 'n12', score: 0.12}), (n13:A {id: 13, name: 'n13', score: 0.13}), (n14:A {id: 14, name: 'n14', score: 0.14}), \
(n15:A {id: 15, name: 'n15', score: 0.15}), (n16:A {id: 16, name: 'n16', score: 0.16}), (n17:A {id: 17, name: 'n17', score: 0.17}), (n18:A {id: 18, name: 'n18', score: 0.18}), (n19:A {id: 19, name: 'n19', score: 0.19}), \
(n20:B {id: 20, name: 'n20', score: 0.20}), (n21:B {id: 21, name: 'n21', score: 0.21}), (n22:B {id: 22, name: 'n22', score: 0.22}), (n23:B {id: 23, name: 'n23', score: 0.23}), (n24:B {id: 24, name: 'n24', score: 0.24}), \
(n25:B {id: 25, name: 'n25', score: 0.25}), (n26:B {id: 26, name: 'n26', score: 0.26}), (n27:B {id: 27, name: 'n27', score: 0.27}), (n28:B {id: 28, name: 'n28', score: 0.28}), (n29:B {id: 29, name: 'n29', score: 0.29}), \
(n30:B {id: 30, name: 'n30', score: 0.30}), (n31:B {id: 31, name: 'n31', score: 0.31}), (n32:B {id: 32, name: 'n32', score: 0.32}), (n33:B {id: 33, name: 'n33', score: 0.33}), (n34:B {id: 34, name: 'n34', score: 0.34}), \
(n35:B {id: 35, name: 'n35', score: 0.35}), (n36:B {id: 36, name: 'n36', score: 0.36}), (n37:B {id: 37, name: 'n37', score: 0.37}), (n38:B {id: 38, name: 'n38', score: 0.38}), (n39:B {id: 39, name: 'n39', score: 0.39}), \
(n40:C {id: 40, name: 'n40', score: 0.40}), (n41:C {id: 41, name: 'n41', score: 0.41}), (n42:C {id: 42, name: 'n42', score: 0.42}), (n43:C {id: 43, name: 'n43', score: 0.43}), (n44:C {id: 44, name: 'n44', score: 0.44}), \
(n45:C {id: 45, name: 'n45', score: 0.45}), (n46:C {id: 46, name: 'n46', score: 0.46}), (n47:C {id: 47, name: 'n47', score: 0.47}), (n48:C {id: 48, name: 'n48', score: 0.48}), (n49:C {id: 49, name: 'n49', score: 0.49}), \
(n50:C {id: 50, name: 'n50', score: 0.50}), (n51:C {id: 51, name: 'n51', score: 0.51}), (n52:C {id: 52, name: 'n52', score: 0.52}), (n53:C {id: 53, name: 'n53', score: 0.53}), (n54:C {id: 54, name: 'n54', score: 0.54}), \
(n55:C {id: 55, name: 'n55', score: 0.55}), (n56:C {id: 56, name: 'n56', score: 0.56}), (n57:C {id: 57, name: 'n57', score: 0.57}), (n58:C {id: 58, name: 'n58', score: 0.58}), (n59:C {id: 59, name: 'n59', score: 0.59}), \
(n60:D {id: 60, name: 'n60', score: 0.60}), (n61:D {id: 61, name: 'n61', score: 0.61}), (n62:D {id: 62, name: 'n62', score: 0.62}), (n63:D {id: 63, name: 'n63', score: 0.63}), (n64:D {id: 64, name: 'n64', score: 0.64}), \
(n65:D {id: 65, name: 'n65', score: 0.65}), (n66:D {id: 66, name: 'n66', score: 0.66}), (n67:D {id: 67, name: 'n67', score: 0.67}), (n68:D {id: 68, name: 'n68', score: 0.68}), (n69:D {id: 69, name: 'n69', score: 0.69}), \
(n70:D {id: 70, name: 'n70', score: 0.70}), (n71:D {id: 71, name: 'n71', score: 0.71}), (n72:D {id: 72, name: 'n72', score: 0.72}), (n73:D {id: 73, name: 'n73', score: 0.73}), (n74:D {id: 74, name: 'n74', score: 0.74}), \
(n75:D {id: 75, name: 'n75', score: 0.75}), (n76:D {id: 76, name: 'n76', score: 0.76}), (n77:D {id: 77, name: 'n77', score: 0.77}), (n78:D {id: 78, name: 'n78', score: 0.78}), (n79:D {id: 79, name: 'n79', score: 0.79}), \
(n80:E {id: 80, name: 'n80', score: 0.80}), (n81:E {id: 81, name: 'n81', score: 0.81}), (n82:E {id: 82, name: 'n82', score: 0.82}), (n83:E {id: 83, name: 'n83', score: 0.83}), (n84:E {id: 84, name: 'n84', score: 0.84}), \
(n85:E {id: 85, name: 'n85', score: 0.85}), (n86:E {id: 86, name: 'n86', score: 0.86}), (n87:E {id: 87, name: 'n87', score: 0.87}), (n88:E {id: 88, name: 'n88', score: 0.88}), (n89:E {id: 89, name: 'n89', score: 0.89}), \
(n90:E {id: 90, name: 'n90', score: 0.90}), (n91:E {id: 91, name: 'n91', score: 0.91}), (n92:E {id: 92, name: 'n92', score: 0.92}), (n93:E {id: 93, name: 'n93', score: 0.93}), (n94:E {id: 94, name: 'n94', score: 0.94}), \
(n95:E {id: 95, name: 'n95', score: 0.95}), (n96:E {id: 96, name: 'n96', score: 0.96}), (n97:E {id: 97, name: 'n97', score: 0.97}), (n98:E {id: 98, name: 'n98', score: 0.98}), (n99:E {id: 99, name: 'n99', score: 0.99}), \
(n0)-[:KNOWS]->(n1), (n1)-[:KNOWS]->(n2), (n2)-[:KNOWS]->(n3), (n3)-[:KNOWS]->(n4), (n4)-[:KNOWS]->(n5), (n5)-[:KNOWS]->(n6), (n6)-[:KNOWS]->(n7), (n7)-[:KNOWS]->(n8), (n8)-[:KNOWS]->(n9), (n9)-[:KNOWS]->(n10), \
(n10)-[:KNOWS]->(n11), (n11)-[:KNOWS]->(n12), (n12)-[:KNOWS]->(n13), (n13)-[:KNOWS]->(n14), (n14)-[:KNOWS]->(n15), (n15)-[:KNOWS]->(n16), (n16)-[:KNOWS]->(n17), (n17)-[:KNOWS]->(n18), (n18)-[:KNOWS]->(n19), (n19)-[:KNOWS]->(n20), \
(n20)-[:KNOWS]->(n21), (n21)-[:KNOWS]->(n22), (n22)-[:KNOWS]->(n23), (n23)-[:KNOWS]->(n24), (n24)-[:KNOWS]->(n25), (n25)-[:KNOWS]->(n26), (n26)-[:KNOWS]->(n27), (n27)-[:KNOWS]->(n28), (n28)-[:KNOWS]->(n29), (n29)-[:KNOWS]->(n30), \
(n30)-[:KNOWS]->(n31), (n31)-[:KNOWS]->(n32), (n32)-[:KNOWS]->(n33), (n33)-[:KNOWS]->(n34), (n34)-[:KNOWS]->(n35), (n35)-[:KNOWS]->(n36), (n36)-[:KNOWS]->(n37), (n37)-[:KNOWS]->(n38), (n38)-[:KNOWS]->(n39), (n39)-[:KNOWS]->(n40), \
(n40)-[:KNOWS]->(n41), (n41)-[:KNOWS]->(n42), (n42)-[:KNOWS]->(n43), (n43)-[:KNOWS]->(n44), (n44)-[:KNOWS]->(n45), (n45)-[:KNOWS]->(n46), (n46)-[:KNOWS]->(n47), (n47)-[:KNOWS]->(n48), (n48)-[:KNOWS]->(n49), (n49)-[:KNOWS]->(n50)";

/// 50-node "small" dataset. All nodes share the `P` label with an
/// `id` property (`p0`..`p49`). Edges form a **hub-plus-chain**
/// topology over a single `KNOWS` type:
///
/// * chain: `p0 → p1 → p2 → … → p49` (49 edges)
/// * hub branches: `p0 → p10`, `p0 → p20`, `p0 → p30`, `p0 → p40`
///   (4 extra edges out of p0)
///
/// Total: 50 nodes, 53 relationships. Every query in the
/// traversal family has a predictable answer — `MATCH
/// (:P {id:0})-[:KNOWS]->(n)` returns 5 rows; the two-hop
/// distinct set is 5; `shortestPath` from p0 to p49 is 10 hops
/// via the hub; `*1..3` from p0 reaches 13 distinct nodes.
///
/// Deliberately minimal: labels beyond `P` and a property graph
/// beyond `id` only add literal weight without improving the
/// traversal story. Richer fixtures belong in future datasets.
pub struct SmallDataset;

impl Dataset for SmallDataset {
    fn kind(&self) -> DatasetKind {
        DatasetKind::Small
    }

    fn name(&self) -> &'static str {
        "small"
    }

    fn node_count(&self) -> usize {
        50
    }

    fn rel_count(&self) -> usize {
        53
    }

    fn load_statement(&self) -> &'static str {
        SMALL_LOAD_STATEMENT
    }
}

/// One Cypher statement — 50 node literals + a 53-edge hub-plus-
/// chain `KNOWS` graph. Variables `p0`..`p49` are bound so the
/// edge section can reference them in the same CREATE.
const SMALL_LOAD_STATEMENT: &str = "CREATE \
(p0:P {id: 0}), (p1:P {id: 1}), (p2:P {id: 2}), (p3:P {id: 3}), (p4:P {id: 4}), (p5:P {id: 5}), (p6:P {id: 6}), (p7:P {id: 7}), (p8:P {id: 8}), (p9:P {id: 9}), \
(p10:P {id: 10}), (p11:P {id: 11}), (p12:P {id: 12}), (p13:P {id: 13}), (p14:P {id: 14}), (p15:P {id: 15}), (p16:P {id: 16}), (p17:P {id: 17}), (p18:P {id: 18}), (p19:P {id: 19}), \
(p20:P {id: 20}), (p21:P {id: 21}), (p22:P {id: 22}), (p23:P {id: 23}), (p24:P {id: 24}), (p25:P {id: 25}), (p26:P {id: 26}), (p27:P {id: 27}), (p28:P {id: 28}), (p29:P {id: 29}), \
(p30:P {id: 30}), (p31:P {id: 31}), (p32:P {id: 32}), (p33:P {id: 33}), (p34:P {id: 34}), (p35:P {id: 35}), (p36:P {id: 36}), (p37:P {id: 37}), (p38:P {id: 38}), (p39:P {id: 39}), \
(p40:P {id: 40}), (p41:P {id: 41}), (p42:P {id: 42}), (p43:P {id: 43}), (p44:P {id: 44}), (p45:P {id: 45}), (p46:P {id: 46}), (p47:P {id: 47}), (p48:P {id: 48}), (p49:P {id: 49}), \
(p0)-[:KNOWS]->(p1), (p1)-[:KNOWS]->(p2), (p2)-[:KNOWS]->(p3), (p3)-[:KNOWS]->(p4), (p4)-[:KNOWS]->(p5), (p5)-[:KNOWS]->(p6), (p6)-[:KNOWS]->(p7), (p7)-[:KNOWS]->(p8), (p8)-[:KNOWS]->(p9), (p9)-[:KNOWS]->(p10), \
(p10)-[:KNOWS]->(p11), (p11)-[:KNOWS]->(p12), (p12)-[:KNOWS]->(p13), (p13)-[:KNOWS]->(p14), (p14)-[:KNOWS]->(p15), (p15)-[:KNOWS]->(p16), (p16)-[:KNOWS]->(p17), (p17)-[:KNOWS]->(p18), (p18)-[:KNOWS]->(p19), (p19)-[:KNOWS]->(p20), \
(p20)-[:KNOWS]->(p21), (p21)-[:KNOWS]->(p22), (p22)-[:KNOWS]->(p23), (p23)-[:KNOWS]->(p24), (p24)-[:KNOWS]->(p25), (p25)-[:KNOWS]->(p26), (p26)-[:KNOWS]->(p27), (p27)-[:KNOWS]->(p28), (p28)-[:KNOWS]->(p29), (p29)-[:KNOWS]->(p30), \
(p30)-[:KNOWS]->(p31), (p31)-[:KNOWS]->(p32), (p32)-[:KNOWS]->(p33), (p33)-[:KNOWS]->(p34), (p34)-[:KNOWS]->(p35), (p35)-[:KNOWS]->(p36), (p36)-[:KNOWS]->(p37), (p37)-[:KNOWS]->(p38), (p38)-[:KNOWS]->(p39), (p39)-[:KNOWS]->(p40), \
(p40)-[:KNOWS]->(p41), (p41)-[:KNOWS]->(p42), (p42)-[:KNOWS]->(p43), (p43)-[:KNOWS]->(p44), (p44)-[:KNOWS]->(p45), (p45)-[:KNOWS]->(p46), (p46)-[:KNOWS]->(p47), (p47)-[:KNOWS]->(p48), (p48)-[:KNOWS]->(p49), \
(p0)-[:KNOWS]->(p10), (p0)-[:KNOWS]->(p20), (p0)-[:KNOWS]->(p30), (p0)-[:KNOWS]->(p40)";

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
        for l in [":A {", ":B {", ":C {", ":D {", ":E {"] {
            assert!(s.contains(l), "label {l} missing from literal");
        }
    }

    #[test]
    fn tiny_load_has_fifty_knows_edges() {
        // rel_count() promises 50 KNOWS edges; verify the literal
        // actually spells them out. A mismatch would make every
        // traversal scenario return 0 silently.
        let s = TinyDataset.load_statement();
        let edge_count = s.matches("-[:KNOWS]->").count();
        assert_eq!(
            edge_count, 50,
            "expected 50 KNOWS edges, found {edge_count}"
        );
    }

    #[test]
    fn tiny_load_binds_every_node_variable() {
        // Every node in the literal must bind a `nN:` variable so
        // the edge section in the same CREATE can reference it.
        let s = TinyDataset.load_statement();
        for id in 0..100 {
            let needle = format!("(n{id}:");
            assert!(
                s.contains(&needle),
                "node variable {needle} missing from literal"
            );
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
        let _: &dyn Dataset = &SmallDataset;
    }

    // ── SmallDataset ────────────────────────────────────────────

    #[test]
    fn small_metadata_matches_literal() {
        let d = SmallDataset;
        assert_eq!(d.kind(), DatasetKind::Small);
        assert_eq!(d.name(), "small");
        assert_eq!(d.node_count(), 50);
        assert_eq!(d.rel_count(), 53);
    }

    #[test]
    fn small_load_is_single_statement() {
        let s = SmallDataset.load_statement();
        // One `CREATE` — same guard the TinyDataset literal carries.
        assert_eq!(s.matches("CREATE ").count(), 1, "must be a single CREATE");
        assert!(!s.contains("MATCH"));
    }

    #[test]
    fn small_load_has_fifty_three_knows_edges() {
        let s = SmallDataset.load_statement();
        let edge_count = s.matches("-[:KNOWS]->").count();
        assert_eq!(
            edge_count, 53,
            "expected 53 KNOWS edges, found {edge_count}"
        );
    }

    #[test]
    fn small_load_binds_every_node_variable() {
        let s = SmallDataset.load_statement();
        for id in 0..50 {
            let needle = format!("(p{id}:");
            assert!(
                s.contains(&needle),
                "node variable {needle} missing from literal"
            );
        }
    }

    #[test]
    fn small_load_fits_in_request_body_budget() {
        let s = SmallDataset.load_statement();
        // Upper bound mirrors TinyDataset's guard; the actual value
        // is ~2 KiB, so a bloated edit trips this well before the
        // server's default body limit would.
        assert!(s.len() < 16 * 1024, "literal is {} bytes", s.len());
    }

    #[test]
    fn small_load_is_uniform_label() {
        // Every node carries `:P` and no other label; scenarios
        // targeting this dataset can rely on that.
        let s = SmallDataset.load_statement();
        // 50 node literals should all contain `:P {`.
        assert_eq!(s.matches(":P {").count(), 50, "all 50 nodes must carry :P");
    }

    #[test]
    fn kind_serde_snake_case_small() {
        let s = serde_json::to_string(&DatasetKind::Small).unwrap();
        assert_eq!(s, "\"small\"");
    }
}
