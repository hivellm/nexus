//! Query-level plan hints extracted from `/*+ ... */` comments.
//!
//! Bench + test callers can steer the columnar fast path on or off
//! without touching `ExecutorConfig` by embedding an SQL-style hint
//! comment in the query text:
//!
//! ```cypher
//! /*+ PREFER_COLUMNAR */  MATCH (n:Person) WHERE n.age > 30 RETURN n
//! /*+ DISABLE_COLUMNAR */ MATCH (n:Person) WHERE n.age > 30 RETURN n
//! ```
//!
//! Unrecognised `/*+ ... */` blocks are left intact so future hints
//! land without breaking today's queries; unknown tokens inside a
//! recognised block are ignored.
//!
//! This is the `cypher::preparse::Hint` touchpoint from
//! `phase3_executor-columnar-wiring` §5.2 — the `cypher` namespace
//! doesn't exist yet in this crate, so the preparse lives under the
//! planner (which already owns the query-shape decisions the hint is
//! meant to steer).

/// A directive parsed out of the query text that overrides a planner
/// / operator default. Extend this enum as new hint tokens land — add
/// the variant here, teach [`extract_plan_hints`] to recognise its
/// token, and consumers that already iterate `Vec<PlanHint>` will
/// pick it up automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanHint {
    /// `/*+ PREFER_COLUMNAR */` → `true`
    ///   Force the columnar fast path for filter / aggregate
    ///   regardless of the `columnar_threshold` row-count check.
    ///
    /// `/*+ DISABLE_COLUMNAR */` → `false`
    ///   Force the row-at-a-time path, even when the batch is big
    ///   enough to amortise columnar materialisation.
    PreferColumnar(bool),
}

/// Scan `query` for recognised `/*+ TOKEN */` hint comments, return
/// the query with every recognised hint stripped plus the `PlanHint`
/// sequence in the order they appeared.
///
/// The stripped query is what the main parser sees — that keeps the
/// hint syntax invisible to the rest of the Cypher front-end. Hints
/// that aren't recognised stay in the text untouched so the parser
/// can still see them (they'll be treated as ordinary block comments
/// today and ignored there).
pub fn extract_plan_hints(query: &str) -> (String, Vec<PlanHint>) {
    let mut hints = Vec::new();
    let mut out = String::with_capacity(query.len());
    let mut remaining = query;

    // Scan for `/*+` openers. The hint syntax is ASCII-only, so
    // byte-position arithmetic via `find` is UTF-8-safe: `find`
    // returns byte indices at `str` boundaries, and we slice there.
    while let Some(start) = remaining.find("/*+") {
        out.push_str(&remaining[..start]);
        let after_open = &remaining[start..];
        if let Some(close_rel) = after_open.find("*/") {
            // `after_open[close_rel..close_rel+2] == "*/"`.
            let body = after_open[3..close_rel].trim();
            let token = body.to_ascii_uppercase();
            match token.as_str() {
                "PREFER_COLUMNAR" => {
                    hints.push(PlanHint::PreferColumnar(true));
                    remaining = &after_open[close_rel + 2..];
                    continue;
                }
                "DISABLE_COLUMNAR" => {
                    hints.push(PlanHint::PreferColumnar(false));
                    remaining = &after_open[close_rel + 2..];
                    continue;
                }
                _ => {
                    // Unknown token — pass the whole `/*+…*/` block
                    // through so the main parser sees it as a plain
                    // block comment.
                    out.push_str(&after_open[..close_rel + 2]);
                    remaining = &after_open[close_rel + 2..];
                    continue;
                }
            }
        } else {
            // Unterminated — preserve the rest verbatim and stop.
            out.push_str(after_open);
            return (out, hints);
        }
    }
    out.push_str(remaining);
    (out, hints)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_prefer_columnar() {
        let (cleaned, hints) = extract_plan_hints("/*+ PREFER_COLUMNAR */ MATCH (n) RETURN n");
        assert_eq!(hints, vec![PlanHint::PreferColumnar(true)]);
        assert_eq!(cleaned.trim_start(), "MATCH (n) RETURN n");
    }

    #[test]
    fn extracts_disable_columnar() {
        let (cleaned, hints) = extract_plan_hints("/*+ DISABLE_COLUMNAR */MATCH (n) RETURN n");
        assert_eq!(hints, vec![PlanHint::PreferColumnar(false)]);
        assert_eq!(cleaned, "MATCH (n) RETURN n");
    }

    #[test]
    fn recognises_token_case_insensitively() {
        let (_, hints) = extract_plan_hints("/*+ prefer_columnar */ MATCH (n) RETURN n");
        assert_eq!(hints, vec![PlanHint::PreferColumnar(true)]);
    }

    #[test]
    fn trims_whitespace_inside_the_block() {
        let (_, hints) = extract_plan_hints("/*+    PREFER_COLUMNAR    */ MATCH (n) RETURN n");
        assert_eq!(hints, vec![PlanHint::PreferColumnar(true)]);
    }

    #[test]
    fn leaves_unknown_hints_in_place() {
        let query = "/*+ USE_INDEX foo */ MATCH (n) RETURN n";
        let (cleaned, hints) = extract_plan_hints(query);
        assert!(hints.is_empty());
        assert_eq!(cleaned, query, "unknown hint must not be stripped");
    }

    #[test]
    fn unterminated_hint_is_not_stripped() {
        let query = "/*+ PREFER_COLUMNAR MATCH (n) RETURN n";
        let (cleaned, hints) = extract_plan_hints(query);
        assert!(hints.is_empty());
        assert_eq!(cleaned, query);
    }

    #[test]
    fn plain_block_comments_are_preserved() {
        let query = "/* regular comment */ MATCH (n) RETURN n";
        let (cleaned, hints) = extract_plan_hints(query);
        assert!(hints.is_empty());
        assert_eq!(cleaned, query);
    }

    #[test]
    fn multiple_hints_in_one_query() {
        let query = "/*+ PREFER_COLUMNAR */ /*+ DISABLE_COLUMNAR */ MATCH (n) RETURN n";
        let (cleaned, hints) = extract_plan_hints(query);
        assert_eq!(
            hints,
            vec![
                PlanHint::PreferColumnar(true),
                PlanHint::PreferColumnar(false),
            ]
        );
        assert_eq!(cleaned.trim(), "MATCH (n) RETURN n");
    }
}
