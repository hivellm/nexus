//! `db.indexes` / `db.indexDetails` and `db.constraints` — index and
//! constraint introspection procedures.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use super::super::super::types::Row;
use crate::{Error, Result};
use serde_json::Value;

impl Executor {
    // ─────────────────────────────────────────────────────────────────────
    // phase6_opencypher-system-procedures §4 — `db.indexes` / `db.indexDetails`
    // ─────────────────────────────────────────────────────────────────────

    /// Row shape matches Neo4j 5.x so drivers deserialise without surprise.
    /// Column order: `id, name, state, populationPercent, uniqueness, type,
    /// entityType, labelsOrTypes, properties, indexProvider`.
    pub(in crate::executor) fn execute_db_indexes_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
        filter_name: Option<&str>,
    ) -> Result<()> {
        let mut rows: Vec<Row> = Vec::new();
        let mut next_id: i64 = 0;

        // Nexus always keeps a label bitmap per label — expose each as an
        // implicit LOOKUP index so `db.indexes()` reports the same schema
        // surface Neo4j does (where every label has a token-lookup index).
        // Iterating the catalog's labels is cheap and includes only
        // user-created labels (not internal).
        for (_label_id, label_name) in self.catalog().list_all_labels() {
            let idx_name = format!("index_label_{}", label_name);
            if filter_name.is_some_and(|n| n != idx_name) {
                continue;
            }
            rows.push(Row {
                values: vec![
                    Value::Number(serde_json::Number::from(next_id)),
                    Value::String(idx_name),
                    Value::String("ONLINE".to_string()),
                    Value::Number(
                        serde_json::Number::from_f64(100.0)
                            .unwrap_or_else(|| serde_json::Number::from(100)),
                    ),
                    Value::String("NONUNIQUE".to_string()),
                    Value::String("LOOKUP".to_string()),
                    Value::String("NODE".to_string()),
                    Value::Array(vec![Value::String(label_name.clone())]),
                    Value::Array(Vec::new()),
                    Value::String("token-lookup-1.0".to_string()),
                    Value::Object(serde_json::Map::new()),
                ],
            });
            next_id += 1;
        }

        // A global KNN vector index exists when one has been registered at
        // engine construction; it's not keyed by label/property in this
        // codebase today, so surface it as a single "vector" row with an
        // empty labels/properties list. Drivers treat the empty list as
        // "applies to any node" and render accordingly.
        {
            let knn = self.knn_index();
            let stats = knn.get_stats();
            if stats.total_vectors > 0 {
                let idx_name = "index_vector_global".to_string();
                if filter_name.is_none_or(|n| n == idx_name) {
                    rows.push(Row {
                        values: vec![
                            Value::Number(serde_json::Number::from(next_id)),
                            Value::String(idx_name),
                            Value::String("ONLINE".to_string()),
                            Value::Number(
                                serde_json::Number::from_f64(100.0)
                                    .unwrap_or_else(|| serde_json::Number::from(100)),
                            ),
                            Value::String("NONUNIQUE".to_string()),
                            Value::String("VECTOR".to_string()),
                            Value::String("NODE".to_string()),
                            Value::Array(Vec::new()),
                            Value::Array(Vec::new()),
                            Value::String("hnsw-1.0".to_string()),
                            Value::Object(serde_json::Map::new()),
                        ],
                    });
                    next_id += 1;
                }
            }
        }

        // phase6_opencypher-advanced-types §3.5 — expose every
        // composite B-tree index registered via
        // `CREATE INDEX <name> FOR (n:L) ON (n.p1, n.p2, ...)`.
        // labelsOrTypes is `[label]` and properties is the ordered
        // list of composite keys, matching Neo4j's
        // RANGE-multi-property convention so drivers render the row
        // correctly without format-specific branching.
        if let Some(registry) = self.composite_btree() {
            for (label_id, property_keys, unique, name_opt) in registry.list() {
                let label_name = match self.catalog().get_label_name(label_id) {
                    Ok(Some(n)) => n,
                    _ => continue,
                };
                let idx_name = name_opt.clone().unwrap_or_else(|| {
                    format!("index_composite_{}_{}", label_name, property_keys.join("_"))
                });
                if filter_name.is_some_and(|n| n != idx_name) {
                    continue;
                }
                rows.push(Row {
                    values: vec![
                        Value::Number(serde_json::Number::from(next_id)),
                        Value::String(idx_name),
                        Value::String("ONLINE".to_string()),
                        Value::Number(
                            serde_json::Number::from_f64(100.0)
                                .unwrap_or_else(|| serde_json::Number::from(100)),
                        ),
                        Value::String(if unique { "UNIQUE" } else { "NONUNIQUE" }.to_string()),
                        Value::String("BTREE".to_string()),
                        Value::String("NODE".to_string()),
                        Value::Array(vec![Value::String(label_name)]),
                        Value::Array(property_keys.into_iter().map(Value::String).collect()),
                        Value::String("btree-composite-1.0".to_string()),
                        Value::Object(serde_json::Map::new()),
                    ],
                });
                next_id += 1;
            }
        }

        // phase6_opencypher-fulltext-search §9.1 — surface every
        // registered FTS index through `db.indexes()` with
        // `type = "FULLTEXT"`, `indexProvider = "tantivy-0.22"`.
        if let Some(registry) = self.fulltext_registry() {
            for meta in registry.list() {
                if filter_name.is_some_and(|n| n != meta.name) {
                    continue;
                }
                rows.push(Row {
                    values: vec![
                        Value::Number(serde_json::Number::from(next_id)),
                        Value::String(meta.name.clone()),
                        Value::String("ONLINE".to_string()),
                        Value::Number(
                            serde_json::Number::from_f64(100.0)
                                .unwrap_or_else(|| serde_json::Number::from(100)),
                        ),
                        Value::String("NONUNIQUE".to_string()),
                        Value::String("FULLTEXT".to_string()),
                        Value::String(
                            match meta.entity {
                                crate::index::fulltext_registry::FullTextEntity::Node => "NODE",
                                crate::index::fulltext_registry::FullTextEntity::Relationship => {
                                    "RELATIONSHIP"
                                }
                            }
                            .to_string(),
                        ),
                        Value::Array(
                            meta.labels_or_types
                                .iter()
                                .map(|s| Value::String(s.clone()))
                                .collect(),
                        ),
                        Value::Array(
                            meta.properties
                                .iter()
                                .map(|s| Value::String(s.clone()))
                                .collect(),
                        ),
                        Value::String("tantivy-0.22".to_string()),
                        {
                            let mut opts = serde_json::Map::new();
                            opts.insert(
                                "analyzer".to_string(),
                                Value::String(meta.analyzer.clone()),
                            );
                            Value::Object(opts)
                        },
                    ],
                });
                next_id += 1;
            }
        }

        // phase6_spatial-planner-seek §5 — surface every registered
        // R-tree index with `type = "RTREE"`, `state = "ONLINE"` (the
        // registry has no failed/building states yet — every entry
        // is online once `register_empty` returns).
        for (idx_name, label, property) in self.shared.rtree_registry.definitions() {
            if filter_name.is_some_and(|n| n != idx_name) {
                continue;
            }
            rows.push(Row {
                values: vec![
                    Value::Number(serde_json::Number::from(next_id)),
                    Value::String(idx_name),
                    Value::String("ONLINE".to_string()),
                    Value::Number(
                        serde_json::Number::from_f64(100.0)
                            .unwrap_or_else(|| serde_json::Number::from(100)),
                    ),
                    Value::String("NONUNIQUE".to_string()),
                    Value::String("RTREE".to_string()),
                    Value::String("NODE".to_string()),
                    Value::Array(vec![Value::String(label)]),
                    Value::Array(vec![Value::String(property)]),
                    Value::String("rtree-1.0".to_string()),
                    Value::Object(serde_json::Map::new()),
                ],
            });
            next_id += 1;
        }

        if filter_name.is_some() && rows.is_empty() {
            return Err(Error::CypherExecution(format!(
                "ERR_INDEX_NOT_FOUND: no index named '{}'",
                filter_name.unwrap()
            )));
        }

        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "id".to_string(),
                "name".to_string(),
                "state".to_string(),
                "populationPercent".to_string(),
                "uniqueness".to_string(),
                "type".to_string(),
                "entityType".to_string(),
                "labelsOrTypes".to_string(),
                "properties".to_string(),
                "indexProvider".to_string(),
                "options".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────
    // phase6_opencypher-system-procedures §5 — `db.constraints`
    // ─────────────────────────────────────────────────────────────────────

    /// Emits one row per user-declared constraint. Columns:
    /// `id, name, type, entityType, labelsOrTypes, properties, ownedIndex`.
    /// Currently reports UNIQUENESS / NODE_KEY / NODE_PROPERTY_EXISTENCE /
    /// RELATIONSHIP_PROPERTY_EXISTENCE as the catalog exposes them.
    pub(in crate::executor) fn execute_db_constraints_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        let mut rows: Vec<Row> = Vec::new();
        // `get_all_constraints` returns a HashMap<(label_id, key_id),
        // Constraint> keyed by the natural composite — we resolve each
        // id pair back to user-visible names via the catalog. This
        // collapses duplicates and keeps the row order deterministic
        // by sorting on (label_name, key_name).
        let all = self
            .catalog()
            .constraint_manager()
            .read()
            .get_all_constraints()
            .unwrap_or_default();
        let mut pairs: Vec<(u32, u32, crate::catalog::constraints::Constraint)> = all
            .into_iter()
            .map(|((lid, kid), c)| (lid, kid, c))
            .collect();
        pairs.sort_by_key(|(lid, kid, _)| (*lid, *kid));
        for (idx, (label_id, key_id, c)) in pairs.into_iter().enumerate() {
            let label_name = self
                .catalog()
                .get_label_name(label_id)
                .ok()
                .flatten()
                .unwrap_or_else(|| format!("label_{}", label_id));
            let key_name = self
                .catalog()
                .get_key_name(key_id)
                .ok()
                .flatten()
                .unwrap_or_else(|| format!("key_{}", key_id));
            let (kind, entity, owned) = match c.constraint_type {
                crate::catalog::constraints::ConstraintType::Unique => (
                    "UNIQUENESS",
                    "NODE",
                    Some(format!("index_unique_{}_{}", label_name, key_name)),
                ),
                crate::catalog::constraints::ConstraintType::Exists => {
                    ("NODE_PROPERTY_EXISTENCE", "NODE", None)
                }
            };
            let name = format!(
                "constraint_{}_{}_{}",
                kind.to_lowercase(),
                label_name,
                key_name
            );
            rows.push(Row {
                values: vec![
                    Value::Number(serde_json::Number::from(idx as i64)),
                    Value::String(name),
                    Value::String(kind.to_string()),
                    Value::String(entity.to_string()),
                    Value::Array(vec![Value::String(label_name)]),
                    Value::Array(vec![Value::String(key_name)]),
                    owned.map(Value::String).unwrap_or(Value::Null),
                ],
            });
        }
        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "id".to_string(),
                "name".to_string(),
                "type".to_string(),
                "entityType".to_string(),
                "labelsOrTypes".to_string(),
                "properties".to_string(),
                "ownedIndex".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }
}
