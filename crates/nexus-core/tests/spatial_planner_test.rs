//! Planner regression tests for the spatial-seek rewriter
//! (phase6_spatial-planner-seek §2 + §3).
//!
//! These tests exercise the recogniser and the cost-based picker
//! end-to-end through the planner: a query that fits one of the
//! seek shapes is planned twice — once with an R-tree registry
//! that contains the index, once with an empty registry — and the
//! resulting operator pipeline is asserted to differ in shape but
//! produce identical correct results when run through the engine.

use nexus_core::catalog::Catalog;
use nexus_core::executor::parser::CypherParser;
use nexus_core::executor::planner::QueryPlanner;
use nexus_core::executor::types::{Operator, SeekMode};
use nexus_core::index::rtree::RTreeRegistry;
use nexus_core::index::{KnnIndex, LabelIndex};
use std::sync::Arc;

/// Build an empty catalog + label index + KNN + R-tree registry
/// scaffold for plan-shape tests. Returns the registry pre-loaded
/// with `points_per_index` synthetic entries against `Place.loc`
/// so the cost picker (§3) reliably prefers the seek over the
/// label scan.
fn planner_scaffold(
    points_per_index: u64,
) -> (
    tempfile::TempDir,
    Catalog,
    LabelIndex,
    KnnIndex,
    Arc<RTreeRegistry>,
) {
    let tmp = tempfile::TempDir::new().unwrap();
    let catalog = Catalog::new(tmp.path()).unwrap();
    let _ = catalog.get_or_create_label("Place").unwrap();
    let label_index = LabelIndex::new();
    let knn = KnnIndex::new_default(384).unwrap();
    let registry = Arc::new(RTreeRegistry::new());
    registry.register_empty("Place.loc");
    for i in 0..points_per_index {
        registry.insert_point("Place.loc", i, i as f64, i as f64);
    }
    (tmp, catalog, label_index, knn, registry)
}

fn plan(
    catalog: &Catalog,
    label_index: &LabelIndex,
    knn: &KnnIndex,
    registry: Option<Arc<RTreeRegistry>>,
    cypher: &str,
) -> Vec<Operator> {
    let mut parser = CypherParser::new(cypher.to_string());
    let ast = parser.parse().unwrap();
    let mut planner = QueryPlanner::new(catalog, label_index, knn);
    if let Some(r) = registry {
        planner = planner.with_rtree(r);
    }
    planner.plan_query(&ast).unwrap()
}

/// §2.1 + §3 — `WHERE point.withinBBox(<var>.<prop>, <bbox-literal>)`
/// rewrites into `Operator::SpatialSeek { mode: Bbox, .. }` when an
/// R-tree index exists for `(label, property)`. The legacy
/// `NodeByLabel` driving operator is replaced.
#[test]
fn planner_rewrites_within_bbox_to_spatial_seek_when_index_exists() {
    let (_tmp, catalog, label_index, knn, registry) = planner_scaffold(1000);
    let cypher = "MATCH (p:Place) \
                  WHERE point.withinBBox(p.loc, {bottomLeft: point({x: 0.0, y: 0.0}), \
                                                  topRight:   point({x: 2.0, y: 2.0})}) \
                  RETURN p";
    let ops = plan(&catalog, &label_index, &knn, Some(registry), cypher);
    assert!(
        ops.iter().any(|o| matches!(
            o,
            Operator::SpatialSeek {
                mode: SeekMode::Bbox { .. },
                ..
            }
        )),
        "expected SpatialSeek::Bbox in {ops:?}"
    );
    // The driving `NodeByLabel` for `:Place` must be gone.
    assert!(
        !ops.iter()
            .any(|o| matches!(o, Operator::NodeByLabel { variable, .. } if variable == "p")),
        "NodeByLabel(:Place) must be replaced: {ops:?}"
    );
}

/// §2.5 — when no R-tree index exists, the rewriter is a no-op
/// and the legacy `NodeByLabel + Filter` plan stands.
#[test]
fn planner_keeps_legacy_plan_when_no_rtree_index() {
    let (_tmp, catalog, label_index, knn, _registry) = planner_scaffold(0);
    let cypher = "MATCH (p:Place) \
                  WHERE point.withinBBox(p.loc, {bottomLeft: point({x: 0.0, y: 0.0}), \
                                                  topRight:   point({x: 2.0, y: 2.0})}) \
                  RETURN p";
    let ops = plan(&catalog, &label_index, &knn, None, cypher);
    assert!(
        !ops.iter()
            .any(|o| matches!(o, Operator::SpatialSeek { .. })),
        "without an rtree handle the planner must NOT emit SpatialSeek: {ops:?}"
    );
    assert!(
        ops.iter()
            .any(|o| matches!(o, Operator::NodeByLabel { variable, .. } if variable == "p")),
        "legacy plan must keep NodeByLabel(:Place): {ops:?}"
    );
}

/// §2.2 — `WHERE point.withinDistance(<var>.<prop>, <pt-lit>, <d-lit>)`
/// rewrites into `Operator::SpatialSeek { mode: WithinDistance, .. }`.
#[test]
fn planner_rewrites_within_distance_to_spatial_seek() {
    let (_tmp, catalog, label_index, knn, registry) = planner_scaffold(1000);
    let cypher = "MATCH (p:Place) \
                  WHERE point.withinDistance(p.loc, point({x: 0.0, y: 0.0}), 5.0) \
                  RETURN p";
    let ops = plan(&catalog, &label_index, &knn, Some(registry), cypher);
    assert!(
        ops.iter().any(|o| matches!(
            o,
            Operator::SpatialSeek {
                mode: SeekMode::WithinDistance { .. },
                ..
            }
        )),
        "expected SpatialSeek::WithinDistance in {ops:?}"
    );
}

/// §2.3 — `MATCH (n:Label) ... ORDER BY distance(n.prop, <pt-lit>)
/// ASC LIMIT <k>` rewrites into `Operator::SpatialSeek { mode:
/// Nearest, .. }`.
#[test]
fn planner_rewrites_order_by_distance_limit_to_nearest_seek() {
    let (_tmp, catalog, label_index, knn, registry) = planner_scaffold(1000);
    let cypher = "MATCH (p:Place) \
                  RETURN p \
                  ORDER BY distance(p.loc, point({x: 0.0, y: 0.0})) ASC \
                  LIMIT 2";
    let ops = plan(&catalog, &label_index, &knn, Some(registry), cypher);
    assert!(
        ops.iter().any(|o| matches!(
            o,
            Operator::SpatialSeek {
                mode: SeekMode::Nearest { k: 2, .. },
                ..
            }
        )),
        "expected SpatialSeek::Nearest {{ k: 2 }} in {ops:?}"
    );
}

/// §3 cost picker — same query under two registry states must
/// pick the cheaper plan. With a populated R-tree (1 000 points
/// → seek cost ≈ log_127(1 000) + 50 ≈ 51.4 vs scan cost 2 000),
/// SpatialSeek wins. With an empty registry handle, the rewriter
/// is a no-op and `NodeByLabel + Filter` stands.
#[test]
fn planner_cost_picker_chooses_seek_when_cheaper() {
    let (_tmp, catalog, label_index, knn, registry) = planner_scaffold(1000);
    let cypher = "MATCH (p:Place) \
                  WHERE point.withinBBox(p.loc, {bottomLeft: point({x: 0.0, y: 0.0}), \
                                                  topRight:   point({x: 2.0, y: 2.0})}) \
                  RETURN p";

    // No registry → legacy plan.
    let ops_no_idx = plan(&catalog, &label_index, &knn, None, cypher);
    assert!(
        !ops_no_idx
            .iter()
            .any(|op| matches!(op, Operator::SpatialSeek { .. })),
        "without rtree handle the planner must not emit SpatialSeek"
    );

    // With populated registry → seek wins by cost.
    let ops_with_idx = plan(&catalog, &label_index, &knn, Some(registry), cypher);
    assert!(
        ops_with_idx.iter().any(|op| matches!(
            op,
            Operator::SpatialSeek {
                mode: SeekMode::Bbox { .. },
                ..
            }
        )),
        "cost picker must select SpatialSeek::Bbox: {ops_with_idx:?}"
    );
}

/// §5 — `CALL db.indexes()` lists every registered R-tree index
/// with `type = "RTREE"`, `state = "ONLINE"`, the matching label /
/// property arrays, and `indexProvider = "rtree-1.0"`.
#[test]
fn db_indexes_reports_rtree_index_with_online_state() {
    let (mut engine, _ctx) = nexus_core::testing::setup_test_engine().expect("setup engine");

    engine
        .execute_cypher("CREATE SPATIAL INDEX ON :Store(loc)")
        .unwrap();

    let rows = engine.execute_cypher("CALL db.indexes()").unwrap();

    // Default column order from `execute_db_indexes_procedure`:
    // [id, name, state, populationPercent, uniqueness, type,
    //  entityType, labelsOrTypes, properties, indexProvider, options].
    let mut found = false;
    for row in &rows.rows {
        let name = row.values[1].as_str().unwrap_or("");
        if name != "Store.loc" {
            continue;
        }
        found = true;
        assert_eq!(row.values[2].as_str(), Some("ONLINE"));
        assert_eq!(row.values[5].as_str(), Some("RTREE"));
        assert_eq!(row.values[6].as_str(), Some("NODE"));
        let labels: Vec<String> = row.values[7]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(labels, vec!["Store".to_string()]);
        let props: Vec<String> = row.values[8]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(props, vec!["loc".to_string()]);
        assert_eq!(row.values[9].as_str(), Some("rtree-1.0"));
    }
    assert!(
        found,
        "db.indexes() must surface the registered :Store(loc) R-tree index, got rows {:?}",
        rows.rows
    );
}
