//! `select_start_pattern` — start-pattern selection for `plan_execution_strategy`
//! — and `synthesise_anonymous_source_anchors`, which gives a synthetic
//! variable to anonymous anchor nodes that carry a label or property filter
//! so `Expand` gets a `source_var` to constrain traversal (see the doc
//! comment on `synthesise_anonymous_source_anchors` for the full rationale).

use super::*;

impl<'a> QueryPlanner<'a> {
    /// Select the most selective pattern to start execution
    /// Give a synthetic variable to anonymous anchor nodes that would
    /// otherwise leave an Expand / VariableLengthPath with an empty
    /// `source_var`. Without this, `execute_expand` takes the source-less
    /// fallback and scans every relationship of the matching type —
    /// returning every edge in the store instead of only the anchor's
    /// outgoing edges (phase6 bench §1, §2).
    ///
    /// Only synthesises for nodes that
    /// - have no variable,
    /// - carry at least one label or property (so the synthesis is worth
    ///   the NodeByLabel + Filter pair the planner emits), and
    /// - are the immediate predecessor of a Relationship element (i.e.
    ///   they are the source of a hop, not a dangling tail).
    pub(super) fn synthesise_anonymous_source_anchors(pattern: &mut Pattern, counter: &mut usize) {
        let len = pattern.elements.len();
        for idx in 0..len {
            // The anchor must be a source of a relationship: next element is a Rel.
            if idx + 1 >= len {
                continue;
            }
            if !matches!(pattern.elements[idx + 1], PatternElement::Relationship(_)) {
                continue;
            }
            // And it must not itself be the target of a prior relationship —
            // that case is handled by the Expand operator's target_var path.
            if idx > 0 && matches!(pattern.elements[idx - 1], PatternElement::Relationship(_)) {
                continue;
            }
            if let PatternElement::Node(node) = &mut pattern.elements[idx] {
                if node.variable.is_some() {
                    continue;
                }
                let has_filterable = !node.labels.is_empty()
                    || node
                        .properties
                        .as_ref()
                        .map(|m| !m.properties.is_empty())
                        .unwrap_or(false);
                if !has_filterable {
                    continue;
                }
                let name = format!("__anchor_{}", *counter);
                *counter += 1;
                node.variable = Some(name);
            }
        }
    }

    pub(super) fn select_start_pattern<'b>(&self, patterns: &'b [Pattern]) -> Result<&'b Pattern> {
        if patterns.is_empty() {
            return Err(Error::CypherSyntax(
                "No patterns found in query".to_string(),
            ));
        }

        // For MVP, just return the first pattern
        // In a full implementation, we would analyze selectivity
        Ok(&patterns[0])
    }
}
