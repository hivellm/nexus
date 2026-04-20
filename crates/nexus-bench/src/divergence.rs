//! Cross-engine row-set divergence guard. Pure logic — exercised
//! by the comparative CLI after each scenario completes.
//!
//! The count-based guard inside the harness only catches "engine
//! returned the wrong number of rows". It cannot see the case
//! where both engines agree on the row count but disagree on the
//! values — two engines that both return exactly one row of
//! `count(*)` can easily report different numbers if one of them
//! has a bug.
//!
//! [`compare_rows`] closes that gap. It normalises both sides
//! (so a Bolt client that returns columns in hash order does not
//! look different from a MessagePack client that returns them in
//! `RETURN`-clause order) and reports the first row that
//! disagrees.
//!
//! Normalisation:
//!
//! * Within a row, values are sorted by their canonical JSON
//!   serialisation. RETURN-clause order is not preserved by all
//!   transports (see `client::neo4j::row_to_json`), so positional
//!   equality would flag false positives. Order-independent
//!   equality still catches genuine value mismatches, which is
//!   the bug class the guard is meant to surface.
//! * Across rows, the row set is sorted the same way. A scenario
//!   that returns five rows in `MATCH` order on one engine and
//!   hash-index order on the other still compares cleanly.
//! * Floats normalise through `serde_json::to_string`, which
//!   renders `1.0` and `1` differently — intentional. If a
//!   scenario hits an integer-vs-float regression it should
//!   surface, not get silently erased.

use std::cmp::Ordering;

use thiserror::Error;

use crate::client::Row;

/// Result of a cross-engine row-content comparison.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ContentDivergence {
    /// Row counts don't match. Strictly redundant with the
    /// harness's built-in guard, but this path is reached when the
    /// comparative CLI invokes [`compare_rows`] directly (i.e. not
    /// via the harness).
    #[error(
        "row count mismatch: {left_label} returned {left} rows, {right_label} returned {right}"
    )]
    RowCount {
        left_label: String,
        right_label: String,
        left: usize,
        right: usize,
    },

    /// Row counts match but at least one row's normalised content
    /// differs. The reported index is into the *sorted* row set,
    /// not the original — the bench does not try to align rows by
    /// some natural id when the engines disagree.
    #[error(
        "row {index} of {total} diverges after normalisation — {left_label}={left:?} vs {right_label}={right:?}"
    )]
    RowContent {
        index: usize,
        total: usize,
        left_label: String,
        right_label: String,
        left: Row,
        right: Row,
    },
}

/// Compare two row sets after normalisation. Returns `Ok(())` when
/// both sides agree. `left_label` / `right_label` name the engines
/// in the error so the comparative report shows which side drifted.
pub fn compare_rows(
    left_label: &str,
    left: &[Row],
    right_label: &str,
    right: &[Row],
) -> Result<(), ContentDivergence> {
    if left.len() != right.len() {
        return Err(ContentDivergence::RowCount {
            left_label: left_label.to_string(),
            right_label: right_label.to_string(),
            left: left.len(),
            right: right.len(),
        });
    }

    let left_n = normalise_row_set(left);
    let right_n = normalise_row_set(right);

    for (i, (l, r)) in left_n.iter().zip(right_n.iter()).enumerate() {
        if l != r {
            return Err(ContentDivergence::RowContent {
                index: i,
                total: left_n.len(),
                left_label: left_label.to_string(),
                right_label: right_label.to_string(),
                left: l.clone(),
                right: r.clone(),
            });
        }
    }
    Ok(())
}

/// Sort every row's cells by canonical-JSON order, then sort the
/// row set itself the same way. The resulting nested structure is
/// directly comparable between two engines that may not have
/// preserved column / row order.
fn normalise_row_set(rows: &[Row]) -> Vec<Row> {
    let mut normalised: Vec<Row> = rows
        .iter()
        .map(|row| {
            let mut cells = row.clone();
            cells.sort_by(cmp_canonical);
            cells
        })
        .collect();
    normalised.sort_by(|a, b| cmp_row(a, b));
    normalised
}

/// Compare two JSON values by their canonical textual form. Used
/// both for within-row cell ordering and cross-row ordering.
fn cmp_canonical(a: &serde_json::Value, b: &serde_json::Value) -> Ordering {
    // `to_string` is infallible on well-formed `Value`s — they're
    // all JSON-representable. `unwrap_or_default` covers the
    // hypothetical panic path (non-finite float) by producing an
    // empty string, which still gives a stable total order.
    let sa = serde_json::to_string(a).unwrap_or_default();
    let sb = serde_json::to_string(b).unwrap_or_default();
    sa.cmp(&sb)
}

fn cmp_row(a: &[serde_json::Value], b: &[serde_json::Value]) -> Ordering {
    let la = a.len().cmp(&b.len());
    if la != Ordering::Equal {
        return la;
    }
    for (x, y) in a.iter().zip(b.iter()) {
        let o = cmp_canonical(x, y);
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_sets_compare_equal() {
        assert!(compare_rows("a", &[], "b", &[]).is_ok());
    }

    #[test]
    fn row_count_mismatch_surfaces() {
        let err = compare_rows("nexus", &[vec![json!(1)]], "neo4j", &[]).unwrap_err();
        assert!(matches!(err, ContentDivergence::RowCount { .. }));
    }

    #[test]
    fn identical_rows_compare_equal() {
        let a = vec![vec![json!(1), json!("x")], vec![json!(2), json!("y")]];
        let b = a.clone();
        assert!(compare_rows("a", &a, "b", &b).is_ok());
    }

    #[test]
    fn same_cells_different_cell_order_within_row_compare_equal() {
        // Bolt client may hand back cells in a different order
        // than the RPC client; normalisation must paper over that.
        let a = vec![vec![json!(1), json!("x")]];
        let b = vec![vec![json!("x"), json!(1)]];
        assert!(compare_rows("a", &a, "b", &b).is_ok());
    }

    #[test]
    fn same_rows_different_row_order_compare_equal() {
        let a = vec![vec![json!(1)], vec![json!(2)]];
        let b = vec![vec![json!(2)], vec![json!(1)]];
        assert!(compare_rows("a", &a, "b", &b).is_ok());
    }

    #[test]
    fn diverging_cell_value_is_caught() {
        let a = vec![vec![json!(1), json!("x")]];
        let b = vec![vec![json!(1), json!("y")]];
        let err = compare_rows("a", &a, "b", &b).unwrap_err();
        assert!(matches!(err, ContentDivergence::RowContent { .. }));
    }

    #[test]
    fn integer_vs_float_is_caught_on_purpose() {
        // An engine that turns a counted integer into a float is
        // a real regression — the guard should NOT hide it.
        let a = vec![vec![json!(1)]];
        let b = vec![vec![json!(1.0)]];
        let err = compare_rows("a", &a, "b", &b).unwrap_err();
        assert!(matches!(err, ContentDivergence::RowContent { .. }));
    }

    #[test]
    fn nested_array_value_compares_structurally() {
        let a = vec![vec![json!([1, 2, 3])]];
        let b = vec![vec![json!([1, 2, 3])]];
        assert!(compare_rows("a", &a, "b", &b).is_ok());

        let c = vec![vec![json!([1, 2, 4])]];
        assert!(compare_rows("a", &a, "c", &c).is_err());
    }

    #[test]
    fn error_preserves_engine_labels() {
        let err = compare_rows("nexus", &[vec![json!(1)]], "neo4j", &[vec![json!(2)]]).unwrap_err();
        match err {
            ContentDivergence::RowContent {
                left_label,
                right_label,
                ..
            } => {
                assert_eq!(left_label, "nexus");
                assert_eq!(right_label, "neo4j");
            }
            other => panic!("expected RowContent, got {other:?}"),
        }
    }
}
