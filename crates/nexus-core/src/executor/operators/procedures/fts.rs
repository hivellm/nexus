//! Full-text search procedures and auto-populate hooks:
//! `db.index.fulltext.{create,query,drop,awaitRefresh,listAnalyzers}` and
//! `fts_autopopulate_node` used by CREATE paths.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use super::super::super::parser;
use super::super::super::types::Row;
use crate::{Error, Result};
use serde_json::Value;

impl Executor {
    // ──────────── phase6_opencypher-fulltext-search procedures ────────────

    pub(in crate::executor) fn fulltext_registry(
        &self,
    ) -> Option<&crate::index::fulltext_registry::FullTextRegistry> {
        self.shared.fulltext()
    }

    /// phase6_fulltext-wal-integration §4 — auto-populate every
    /// registered FTS index whose label/property set matches the
    /// node just created. Called from the CREATE operators' node-
    /// creation paths.
    ///
    /// Match rule: a node is indexed by a given FTS index when
    /// (a) it carries at least one of the index's labels AND
    /// (b) at least one of the index's properties has a string
    /// value on the node. The indexed content is the whitespace-
    /// joined concatenation of every matching string property in
    /// the order the index declared them.
    ///
    /// Errors from individual FTS writes do NOT abort the caller —
    /// FTS is an index, not a source of truth. Failures surface via
    /// `tracing::warn!` so the CREATE path stays durable even when
    /// one Tantivy index is misbehaving.
    pub(in crate::executor) fn fts_autopopulate_node(
        &self,
        node_id: u64,
        label_ids: &[u32],
        properties: &serde_json::Value,
    ) {
        use crate::index::fulltext_registry::FullTextEntity;
        let Some(registry) = self.fulltext_registry() else {
            return;
        };
        let Some(props_obj) = properties.as_object() else {
            return;
        };
        for meta in registry.list() {
            if meta.entity != FullTextEntity::Node {
                continue;
            }
            let mut matches_label = false;
            for label_name in &meta.labels_or_types {
                if let Ok(id) = self.catalog().get_label_id(label_name) {
                    if label_ids.contains(&id) {
                        matches_label = true;
                        break;
                    }
                }
            }
            if !matches_label {
                continue;
            }
            let mut parts: Vec<String> = Vec::new();
            for prop in &meta.properties {
                if let Some(v) = props_obj.get(prop) {
                    if let Some(s) = v.as_str() {
                        parts.push(s.to_string());
                    }
                }
            }
            if parts.is_empty() {
                continue;
            }
            let content = parts.join(" ");
            if let Err(e) = registry.add_node_document(&meta.name, node_id, 0, 0, &content) {
                tracing::warn!(
                    "FTS: autopopulate on index {:?} for node {node_id} failed: {e}",
                    meta.name
                );
            }
        }
    }

    pub(in crate::executor) fn execute_fts_create(
        &self,
        context: &mut ExecutionContext,
        arguments: &[parser::Expression],
        yield_columns: Option<&Vec<String>>,
        is_node: bool,
    ) -> Result<()> {
        let name = self.fts_str_arg(context, arguments, 0, "name")?;
        let labels = self.fts_str_list_arg(context, arguments, 1, "labelsOrTypes")?;
        let props = self.fts_str_list_arg(context, arguments, 2, "properties")?;
        let config = self.fts_parse_analyzer_config(context, arguments, 3)?;
        let registry = self.fulltext_registry().ok_or_else(|| {
            Error::CypherExecution(
                "ERR_FTS_INDEX_UNAVAILABLE: registry not configured on this executor".to_string(),
            )
        })?;
        let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
        let prop_refs: Vec<&str> = props.iter().map(|s| s.as_str()).collect();
        if is_node {
            registry.create_node_index_with_config(&name, &label_refs, &prop_refs, config)?;
        } else {
            registry.create_relationship_index_with_config(
                &name,
                &label_refs,
                &prop_refs,
                config,
            )?;
        }
        let columns = yield_columns
            .cloned()
            .unwrap_or_else(|| vec!["name".to_string(), "state".to_string()]);
        context.set_columns_and_rows(
            columns,
            vec![Row {
                values: vec![Value::String(name), Value::String("ONLINE".to_string())],
            }],
        );
        Ok(())
    }

    pub(in crate::executor) fn execute_fts_query(
        &self,
        context: &mut ExecutionContext,
        arguments: &[parser::Expression],
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        let name = self.fts_str_arg(context, arguments, 0, "name")?;
        let query = self.fts_str_arg(context, arguments, 1, "query")?;
        let registry = self.fulltext_registry().ok_or_else(|| {
            Error::CypherExecution(
                "ERR_FTS_INDEX_UNAVAILABLE: registry not configured on this executor".to_string(),
            )
        })?;
        let results = registry.query(&name, &query, None)?;
        let columns = yield_columns
            .cloned()
            .unwrap_or_else(|| vec!["node".to_string(), "score".to_string()]);
        let rows: Vec<Row> = results
            .into_iter()
            .map(|r| {
                let node = serde_json::json!({
                    "_nexus_id": r.node_id,
                    "value": r.value,
                });
                let score = serde_json::Number::from_f64(r.score as f64)
                    .map(Value::Number)
                    .unwrap_or(Value::Null);
                Row {
                    values: vec![node, score],
                }
            })
            .collect();
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    pub(in crate::executor) fn execute_fts_drop(
        &self,
        context: &mut ExecutionContext,
        arguments: &[parser::Expression],
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        let name = self.fts_str_arg(context, arguments, 0, "name")?;
        let registry = self.fulltext_registry().ok_or_else(|| {
            Error::CypherExecution(
                "ERR_FTS_INDEX_UNAVAILABLE: registry not configured on this executor".to_string(),
            )
        })?;
        let removed = registry.drop_index(&name)?;
        let columns = yield_columns
            .cloned()
            .unwrap_or_else(|| vec!["name".to_string(), "state".to_string()]);
        let state = if removed { "DROPPED" } else { "NOT_FOUND" };
        context.set_columns_and_rows(
            columns,
            vec![Row {
                values: vec![Value::String(name), Value::String(state.to_string())],
            }],
        );
        Ok(())
    }

    pub(in crate::executor) fn execute_fts_await_refresh(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // The registry's writer commits + reloads synchronously on
        // every add_node_document today (ReloadPolicy::Manual with an
        // explicit reload after each commit), so the "await" window
        // is already bounded at zero. Return an acknowledgement row.
        let columns = yield_columns
            .cloned()
            .unwrap_or_else(|| vec!["status".to_string()]);
        context.set_columns_and_rows(
            columns,
            vec![Row {
                values: vec![Value::String("refreshed".to_string())],
            }],
        );
        Ok(())
    }

    pub(in crate::executor) fn execute_fts_list_analyzers(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        let columns = yield_columns
            .cloned()
            .unwrap_or_else(|| vec!["name".to_string(), "description".to_string()]);
        let rows: Vec<Row> = crate::index::fulltext_analyzer::catalogue()
            .into_iter()
            .map(|d| Row {
                values: vec![
                    Value::String(d.name.to_string()),
                    Value::String(d.description.to_string()),
                ],
            })
            .collect();
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    /// Parse the optional `config` map argument of
    /// `db.index.fulltext.createNodeIndex / createRelationshipIndex`
    /// into an [`AnalyzerConfig`]. Supported keys:
    ///
    /// - `analyzer` (STRING): catalogue name; defaults to `"standard"`.
    /// - `ngram_min` (INTEGER): lower bound for the `ngram` analyzer.
    /// - `ngram_max` (INTEGER): upper bound for the `ngram` analyzer.
    ///
    /// Any other keys are ignored (forward-compat with Neo4j
    /// configuration maps that carry additional tuning flags).
    pub(in crate::executor) fn fts_parse_analyzer_config(
        &self,
        context: &ExecutionContext,
        arguments: &[parser::Expression],
        idx: usize,
    ) -> Result<crate::index::fulltext_registry::AnalyzerConfig> {
        use crate::index::fulltext_registry::AnalyzerConfig;
        let Some(expr) = arguments.get(idx) else {
            return Ok(AnalyzerConfig::of_name(None));
        };
        let value = self.evaluate_expression_in_context(context, expr)?;
        let Value::Object(map) = value else {
            // Non-map config is treated as "no config" (NULL or a
            // misuse); the Neo4j procedure signature accepts the map
            // as optional, so surface no failure here.
            return Ok(AnalyzerConfig::of_name(None));
        };
        let name = map
            .get("analyzer")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "standard".to_string());
        let ngram_min = map
            .get("ngram_min")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        let ngram_max = map
            .get("ngram_max")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
        Ok(AnalyzerConfig {
            name,
            ngram_min,
            ngram_max,
        })
    }

    pub(in crate::executor) fn fts_str_arg(
        &self,
        context: &ExecutionContext,
        arguments: &[parser::Expression],
        idx: usize,
        name: &str,
    ) -> Result<String> {
        match arguments.get(idx) {
            Some(expr) => match self.evaluate_expression_in_context(context, expr)? {
                Value::String(s) => Ok(s),
                other => Err(Error::CypherExecution(format!(
                    "ERR_INVALID_ARG_TYPE: db.index.fulltext arg {idx} ({name}) must be STRING \
                     (got {other})",
                ))),
            },
            None => Err(Error::CypherExecution(format!(
                "ERR_MISSING_ARG: db.index.fulltext requires a `{name}` argument at position {idx}",
            ))),
        }
    }

    pub(in crate::executor) fn fts_str_list_arg(
        &self,
        context: &ExecutionContext,
        arguments: &[parser::Expression],
        idx: usize,
        name: &str,
    ) -> Result<Vec<String>> {
        match arguments.get(idx) {
            Some(expr) => match self.evaluate_expression_in_context(context, expr)? {
                Value::Array(arr) => {
                    let mut out = Vec::with_capacity(arr.len());
                    for v in arr {
                        match v {
                            Value::String(s) => out.push(s),
                            other => {
                                return Err(Error::CypherExecution(format!(
                                    "ERR_INVALID_ARG_TYPE: db.index.fulltext {name}[] elements \
                                     must be STRING (got {other})",
                                )));
                            }
                        }
                    }
                    Ok(out)
                }
                other => Err(Error::CypherExecution(format!(
                    "ERR_INVALID_ARG_TYPE: db.index.fulltext arg {idx} ({name}) must be \
                     LIST<STRING> (got {other})",
                ))),
            },
            None => Err(Error::CypherExecution(format!(
                "ERR_MISSING_ARG: db.index.fulltext requires a `{name}` argument at position {idx}",
            ))),
        }
    }
}
