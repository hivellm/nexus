//! Supporting `Executor` operations for [`super::execute`] /
//! [`super::operator_loop`]: intelligent query-cache accessors and the
//! `COUNT(*)` cross-product short-circuit fast paths (including the
//! direct-execution check used before the operator pipeline runs).

use super::super::*;
use crate::Result;
use crate::query_cache::QueryCacheConfig;
use serde_json::Value;

impl Executor {
    /// Enable intelligent query caching with default configuration
    pub fn enable_query_cache(&mut self) -> Result<()> {
        self.shared.enable_query_cache()
    }

    /// Enable intelligent query caching with custom configuration
    pub fn enable_query_cache_with_config(&mut self, config: QueryCacheConfig) -> Result<()> {
        self.shared.enable_query_cache_with_config(config)
    }

    /// Disable query caching
    pub fn disable_query_cache(&mut self) {
        self.shared.query_cache = None;
    }

    /// Clear all cached query results
    pub fn clear_query_cache(&self) {
        if let Some(ref cache) = self.shared.query_cache {
            cache.write().clear();
        }
    }

    /// Get query cache statistics
    pub fn get_query_cache_stats(&self) -> Option<crate::query_cache::QueryCacheStats> {
        self.shared
            .query_cache
            .as_ref()
            .map(|cache| cache.read().stats())
    }

    /// Short-circuit `MATCH (a:L1), (b:L2), ... RETURN count(*)` and
    /// `MATCH (n) RETURN count(n)` (unlabelled scan), plus their
    /// `count(var)` variants when `var` is bound by one of the scans,
    /// with a metadata-driven count instead of materialising every
    /// row. Returns `Some(rs)` if the plan matches; `None` if any
    /// operator disqualifies the short-circuit so the caller falls
    /// back to the full operator-driven execution.
    ///
    /// Qualifying plan shape:
    /// - 1..=N scan operators, each either `Operator::NodeByLabel
    ///   { label_id, variable }` or `Operator::AllNodesScan { variable }`
    ///   (each with a distinct variable).
    /// - exactly one trailing `Operator::Aggregate` with
    ///   `group_by` empty,
    ///   `aggregations` a single `Aggregation::Count { column, distinct: false, .. }`.
    ///   If `column` is `Some(c)`, `c` must be a *bare* variable (no `.`)
    ///   matching one of the scan variables — that's still a row-count.
    ///   A dotted `count(var.prop)` is a property-presence count (NULL /
    ///   missing values are excluded), which is NOT the same number as
    ///   the scan's row count, so it must fall back to the full scan
    ///   (DISTINCT would also change semantics and is explicitly ruled
    ///   out above).
    /// - no other operators between, before, or after.
    ///
    /// Result = product of the live (non-deleted) row count of each
    /// scan. Label counts come from the label bitmap; the unlabelled
    /// scan counts every live node. Both walk record headers only —
    /// no property-chain materialisation — so this stays cheap even
    /// when a shortcut-eligible shape is `AllNodesScan` over the whole
    /// store.
    pub(in super::super) fn try_short_circuit_count_cross_product(
        &self,
        operators: &[Operator],
    ) -> Result<Option<ResultSet>> {
        if operators.len() < 2 {
            return Ok(None);
        }
        let (agg_idx, group_by, aggregations) = match operators.last() {
            Some(Operator::Aggregate {
                group_by,
                aggregations,
                ..
            }) if group_by.is_empty() && aggregations.len() == 1 => {
                (operators.len() - 1, group_by, aggregations)
            }
            _ => return Ok(None),
        };
        let (count_column, count_alias) = match &aggregations[0] {
            Aggregation::Count {
                column,
                alias,
                distinct: false,
            } => (column.clone(), alias.clone()),
            _ => return Ok(None),
        };
        // phase8_neo4j-concurrency-gaps §1 — `RETURN count(<scan var>) AS x`
        // (as opposed to bare `count(*)`) makes the planner insert an
        // identity `Project { <var> -> <var> }` between the scan(s) and
        // the `Aggregate` (it materialises the row `count()` reads from).
        // That extra operator used to disqualify every `count(var)` plan
        // from this short-circuit — including this benchmark's own
        // `aggregation.count_all` scenario (`MATCH (n) RETURN count(n) AS
        // c`) — silently falling through to the full per-node JSON
        // materialisation path (`execute_all_nodes_scan` /
        // `execute_node_by_label`, which load every property + label per
        // node) instead of the header-only count below. That was the
        // actual cause of the 16w-\>64w collapse (2.5k -\> 2.9k qps, p99
        // 124ms): the short-circuit *looked* like it covered this query
        // shape but never actually matched it.
        //
        // A trailing Project is safe to skip over as long as it is a
        // pure passthrough: every item projects a bare variable to
        // itself with no transformation. Such a Project cannot add,
        // remove, or alter rows, so the row count above/below it is
        // identical and short-circuiting past it stays sound.
        let scan_end = if agg_idx >= 2
            && let Operator::Project { items } = &operators[agg_idx - 1]
            && items.iter().all(|item| {
                matches!(&item.expression, parser::Expression::Variable(v) if v == &item.alias)
            }) {
            agg_idx - 1
        } else {
            agg_idx
        };
        // `None` = unlabelled `AllNodesScan`, `Some(label_id)` = `NodeByLabel`.
        let mut scans: Vec<Option<u32>> = Vec::with_capacity(scan_end);
        let mut variables: Vec<String> = Vec::with_capacity(scan_end);
        for op in &operators[..scan_end] {
            match op {
                Operator::NodeByLabel { label_id, variable } => {
                    scans.push(Some(*label_id));
                    variables.push(variable.clone());
                }
                Operator::AllNodesScan { variable } => {
                    scans.push(None);
                    variables.push(variable.clone());
                }
                _ => return Ok(None),
            }
        }
        if scans.is_empty() {
            return Ok(None);
        }
        if let Some(col) = &count_column {
            // A dotted column is a property count, not a row count — see
            // the doc comment above. Only a bare scan variable qualifies.
            if col.contains('.') || !variables.iter().any(|v| v == col) {
                return Ok(None);
            }
        }
        // Count live (non-deleted) rows per scan directly from record
        // headers — cheaper than `execute_node_by_label`'s full
        // property-chain materialisation, and still authoritative
        // (unlike `catalog.get_node_count`, which is an increment-only
        // histogram that DELETE never decrements, see its doc comment).
        let mut product: u64 = 1;
        for scan in &scans {
            let count = match scan {
                Some(label_id) => self.count_live_nodes_for_label(*label_id)?,
                None => self.count_live_nodes_all()?,
            };
            product = match product.checked_mul(count) {
                Some(v) => v,
                None => return Ok(None),
            };
        }
        Ok(Some(ResultSet::new(
            vec![count_alias],
            vec![Row {
                values: vec![Value::Number(serde_json::Number::from(product))],
            }],
        )))
    }

    /// Count live (non-deleted) nodes carrying `label_id` straight from
    /// the label bitmap, reading only each candidate's record header
    /// (`is_deleted`) rather than materialising its full JSON payload
    /// the way [`Self::execute_node_by_label`] does. Used by the
    /// `COUNT(*)` short-circuit above, where only the cardinality
    /// matters.
    ///
    /// phase8_neo4j-concurrency-gaps §1 — reads every node header in a
    /// single [`crate::storage::RecordStore::read_all_node_headers`]
    /// call and indexes into the resulting in-memory snapshot for each
    /// bitmap member, instead of taking a fresh `nodes_mmap` lock per
    /// candidate node. See that method's doc comment for the full
    /// contention analysis (this scenario's 16w-\>64w collapse: 2.5k -\>
    /// 2.9k qps flat, p99 124ms, while Neo4j scaled to 13k).
    pub(in super::super) fn count_live_nodes_for_label(&self, label_id: u32) -> Result<u64> {
        let bitmap = self.label_index().get_nodes(label_id)?;
        let headers = self.store().read_all_node_headers();
        let mut count = 0u64;
        for node_id in bitmap.iter() {
            if let Some(node_record) = headers.get(node_id as usize) {
                if !node_record.is_deleted() {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// Count every live (non-deleted) node in the store by walking
    /// record headers only. Shared by [`Self::execute_count_all_nodes`]
    /// (the `MATCH (n) RETURN count(n)` string-matched fast path) and
    /// the `COUNT(*)` cross-product short-circuit's `AllNodesScan` arm.
    ///
    /// phase8_neo4j-concurrency-gaps §1 — same single bulk-read fix as
    /// [`Self::count_live_nodes_for_label`] above; see its doc comment.
    /// This is the dominant scan in `aggregation.count_all` (`MATCH (n)
    /// RETURN count(n)`), which walks every node in the store on every
    /// call — the scenario this fix targets directly.
    pub(in super::super) fn count_live_nodes_all(&self) -> Result<u64> {
        let headers = self.store().read_all_node_headers();
        Ok(headers.iter().filter(|n| !n.is_deleted()).count() as u64)
    }

    /// Check if query is a simple MATCH query that can be executed directly
    pub(in super::super) fn is_simple_match_query(&self, cypher: &str) -> bool {
        let cypher = cypher.trim();

        // Simple pattern: exactly "MATCH (n) RETURN count(n)" (no alias, no
        // trailing clauses). Must be an EXACT match, not `starts_with`: the
        // direct-execution path below (`execute_count_all_nodes`) hardcodes
        // the output column name to "count", so `MATCH (n) RETURN count(n)
        // AS total` matching here would silently drop the alias. Aliased /
        // widened forms fall through to `try_short_circuit_count_cross_product`,
        // which derives the real alias from the parsed `Aggregate` operator.
        if cypher == "MATCH (n) RETURN count(n)" {
            return true;
        }

        // Simple patterns: "MATCH (n:Person) RETURN n LIMIT X"
        if cypher.contains("MATCH (n:")
            && cypher.contains("RETURN n LIMIT")
            && !cypher.contains("WHERE")
        {
            return true;
        }

        // Simple patterns: "MATCH (n) RETURN n LIMIT X"
        if cypher.starts_with("MATCH (n) RETURN n LIMIT") && !cypher.contains("WHERE") {
            return true;
        }

        false
    }

    /// Execute simple MATCH queries directly (bypass operator planning)
    pub(in super::super) fn execute_simple_match_directly(
        &self,
        query: &Query,
    ) -> Result<ResultSet> {
        let cypher = query.cypher.trim();

        // Only optimize COUNT(*) for now - other queries are better handled by the traditional pipeline
        if cypher.starts_with("MATCH (n) RETURN count(n)") {
            return self.execute_count_all_nodes();
        }

        Err(crate::error::Error::Internal(
            "Not a supported simple query pattern".to_string(),
        ))
    }

    /// Execute COUNT(*) directly from storage
    pub(in super::super) fn execute_count_all_nodes(&self) -> Result<ResultSet> {
        // Count non-deleted nodes directly from storage.
        // This is more reliable than using catalog statistics which may not be updated.
        let count = self.count_live_nodes_all()?;

        let row = Row {
            values: vec![serde_json::Value::Number(count.into())],
        };

        Ok(ResultSet::new(vec!["count".to_string()], vec![row]))
    }

    /// Invalidate cache entries based on affected data
    pub fn invalidate_query_cache(&self, affected_labels: &[&str], affected_properties: &[&str]) {
        if let Some(ref cache) = self.shared.query_cache {
            cache
                .write()
                .invalidate_by_pattern(affected_labels, affected_properties);
        }
    }

    /// Clean expired cache entries
    pub fn clean_query_cache(&self) {
        if let Some(ref cache) = self.shared.query_cache {
            cache.write().clean_expired();
        }
    }
}
