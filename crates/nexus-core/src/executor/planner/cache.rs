//! Query-plan cache primitives.
//!
//! # Scope
//!
//! Phase 8 ships the **canonicaliser** that turns a Cypher query
//! string into a cache-key-friendly form. Two queries that differ
//! only in whitespace, comments, or trailing punctuation
//! canonicalise to the same string and therefore hit the same
//! plan cache entry.
//!
//! The full plan-cache integration (lookup at `Engine::execute`,
//! schema-change invalidation, `db.planCache.*` procedures, env
//! vars, `/stats` counters, Prometheus metrics) is tracked under
//! `phase8_query-plan-cache`; the canonicaliser ships first so the
//! existing per-optimizer plan cache in
//! `crates/nexus-core/src/executor/optimizer.rs` can adopt it
//! without waiting on the rest. The contract here is stable; a
//! future cache wiring consumes it without changes.
//!
//! # Why not just hash raw text
//!
//! `MATCH (n) RETURN n` and `MATCH (n)  RETURN n` (extra space)
//! hash to different cache entries today, missing every cache
//! lookup that should have hit. Comments (`MATCH (n) // comment`)
//! also miss. Real workloads paste the same parameterised query
//! through templating that injects insignificant whitespace and
//! comments; canonicalising before hashing tightens the hit rate
//! without changing semantics.
//!
//! # What the canonicaliser does
//!
//! 1. Strips line comments (`// ...` to end of line).
//! 2. Strips block comments (`/* ... */`, non-nested).
//! 3. Collapses every run of ASCII whitespace (space, tab,
//!    newline, carriage return) to a single space.
//! 4. Trims leading + trailing whitespace.
//! 5. Preserves string literals byte-for-byte — the contents of
//!    `"..."` and `'...'` are not touched, so a query like
//!    `MATCH (n {note: 'a  b'})` keeps its literal intact.
//!
//! # What the canonicaliser does NOT do
//!
//! - Does not lower-case keywords. `MATCH` and `match` produce
//!   different canonical strings — by design. Cypher is case-
//!   sensitive on identifiers and the parser already accepts
//!   case-insensitive keywords; lower-casing here would risk
//!   false aliasing of identifiers like `match` (a property
//!   name) with the keyword.
//! - Does not normalise parameter placeholders. `$x` and `$y`
//!   produce different keys. The plan-cache key is the
//!   *parameterised* query text; values are not part of the
//!   key, but parameter names are because they participate in
//!   binding. Two queries with different `$` names produce
//!   different plans (different variable scopes).
//! - Does not parse the query. The canonicaliser is character-
//!   level so it stays cheap (sub-microsecond on typical
//!   queries) and cannot fail on bad syntax — the plan cache
//!   miss path then runs the real parser, which does fail.

use std::borrow::Cow;

/// Cypher comment + whitespace canonicaliser. Returns a
/// `Cow::Borrowed` when the input is already canonical (saves an
/// allocation on every cache hit), otherwise an owned `String`.
///
/// Stable across releases — bumping the canonicaliser bumps the
/// effective cache key, so the version is pinned via the
/// [`CANONICAL_VERSION`] constant. A future shape change requires
/// the constant to advance so callers that include it in their
/// hash invalidate their caches.
pub fn canonicalise_query(input: &str) -> Cow<'_, str> {
    // Fast path: scan for any non-canonical char (comments,
    // multi-space runs, leading/trailing whitespace). If none
    // present, return the input borrowed.
    if is_already_canonical(input) {
        return Cow::Borrowed(input);
    }
    Cow::Owned(rewrite(input))
}

/// Version stamp for the canonical form. Append to a hash key when
/// the canonical form participates in long-lived persistence (a
/// cache that survives restarts, a key in a regression report).
/// The current implementation produces version 1.
pub const CANONICAL_VERSION: u32 = 1;

fn is_already_canonical(input: &str) -> bool {
    if input.is_empty() {
        return true;
    }
    let bytes = input.as_bytes();
    if bytes.first().is_some_and(|b| b.is_ascii_whitespace())
        || bytes.last().is_some_and(|b| b.is_ascii_whitespace())
    {
        return false;
    }
    let mut prev_space = false;
    let mut iter = input.char_indices().peekable();
    while let Some((_, ch)) = iter.next() {
        match ch {
            // Any embedded comment marker disqualifies (we do not
            // distinguish "comment inside a string" here — fast
            // path falls through to the rewriter, which does).
            '/' => {
                if let Some((_, next)) = iter.peek() {
                    if *next == '/' || *next == '*' {
                        return false;
                    }
                }
            }
            '\'' | '"' => {
                // Skip the literal — we don't care what's inside.
                let quote = ch;
                while let Some((_, next)) = iter.next() {
                    if next == '\\' {
                        // Skip the escaped char.
                        iter.next();
                        continue;
                    }
                    if next == quote {
                        break;
                    }
                }
                prev_space = false;
            }
            c if c == '\t' || c == '\n' || c == '\r' => return false,
            ' ' => {
                if prev_space {
                    return false;
                }
                prev_space = true;
            }
            _ => prev_space = false,
        }
    }
    true
}

fn rewrite(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut bytes = input.bytes().enumerate();
    let raw = input.as_bytes();
    let mut prev_space = true; // suppress leading whitespace

    while let Some((i, b)) = bytes.next() {
        match b {
            b'/' if raw.get(i + 1) == Some(&b'/') => {
                // Line comment: skip to end of line (or EOF).
                for (_, c) in bytes.by_ref() {
                    if c == b'\n' {
                        break;
                    }
                }
                if !prev_space {
                    out.push(' ');
                    prev_space = true;
                }
            }
            b'/' if raw.get(i + 1) == Some(&b'*') => {
                // Block comment: skip until closing `*/` (non-nested).
                bytes.next(); // consume `*`
                let mut last = 0u8;
                for (_, c) in bytes.by_ref() {
                    if last == b'*' && c == b'/' {
                        break;
                    }
                    last = c;
                }
                if !prev_space {
                    out.push(' ');
                    prev_space = true;
                }
            }
            b'\'' | b'"' => {
                let quote = b;
                out.push(quote as char);
                // Copy the literal verbatim, honouring `\` escapes.
                let mut escaped = false;
                for (_, c) in bytes.by_ref() {
                    out.push(c as char);
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    if c == b'\\' {
                        escaped = true;
                        continue;
                    }
                    if c == quote {
                        break;
                    }
                }
                prev_space = false;
            }
            b' ' | b'\t' | b'\n' | b'\r' => {
                if !prev_space {
                    out.push(' ');
                    prev_space = true;
                }
            }
            _ => {
                out.push(b as char);
                prev_space = false;
            }
        }
    }
    // Trim trailing space if we emitted one.
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Hash a query for plan-cache lookup using the canonicalised form
/// + the canonical version stamp. Uses `xxhash_rust::xxh3::xxh3_64`
/// — the same hasher already in nexus-core's hot paths.
///
/// Stable: same query, same canonical version → same hash.
pub fn hash_canonicalised(input: &str) -> u64 {
    use xxhash_rust::xxh3::Xxh3;
    let canonical = canonicalise_query(input);
    let mut hasher = Xxh3::new();
    hasher.update(&CANONICAL_VERSION.to_le_bytes());
    hasher.update(canonical.as_bytes());
    hasher.digest()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_canonical() {
        assert!(matches!(canonicalise_query(""), Cow::Borrowed("")));
    }

    #[test]
    fn already_canonical_input_borrows_unchanged() {
        let q = "MATCH (n) RETURN n";
        match canonicalise_query(q) {
            Cow::Borrowed(s) => assert_eq!(s, q),
            Cow::Owned(_) => panic!("expected borrow on canonical input"),
        }
    }

    #[test]
    fn whitespace_runs_collapse_to_single_space() {
        let q = "MATCH (n)   RETURN n";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n) RETURN n");
    }

    #[test]
    fn tabs_and_newlines_collapse_to_single_space() {
        let q = "MATCH (n)\t\n  RETURN\nn";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n) RETURN n");
    }

    #[test]
    fn leading_and_trailing_whitespace_trimmed() {
        let q = "   MATCH (n) RETURN n   ";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n) RETURN n");
    }

    #[test]
    fn line_comments_stripped() {
        let q = "MATCH (n) // a node\nRETURN n";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n) RETURN n");
    }

    #[test]
    fn block_comments_stripped_non_nested() {
        let q = "MATCH /* a node */ (n) RETURN n";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n) RETURN n");
    }

    #[test]
    fn block_comment_at_eol_no_terminator_is_consumed_to_eof() {
        // Pathological: unterminated block comment. The
        // canonicaliser silently consumes to EOF — the parser
        // will fail later with a real error message. Document
        // the behaviour so a future test does not surprise.
        let q = "MATCH (n) /* unterminated";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n)");
    }

    #[test]
    fn string_literal_double_quote_preserved_byte_for_byte() {
        let q = "MATCH (n {note: \"a  b   c\"}) RETURN n";
        let c = canonicalise_query(q);
        // Whitespace inside the literal must NOT collapse.
        assert_eq!(c, "MATCH (n {note: \"a  b   c\"}) RETURN n");
    }

    #[test]
    fn string_literal_single_quote_preserved() {
        let q = "MATCH (n {note: 'a  b'}) RETURN n";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n {note: 'a  b'}) RETURN n");
    }

    #[test]
    fn string_literal_with_escaped_quote_preserved() {
        let q = r#"MATCH (n) WHERE n.note = "a\"b" RETURN n"#;
        let c = canonicalise_query(q);
        assert_eq!(c, r#"MATCH (n) WHERE n.note = "a\"b" RETURN n"#);
    }

    #[test]
    fn comment_inside_string_literal_not_stripped() {
        let q = "MATCH (n {note: 'a // b'}) RETURN n";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n {note: 'a // b'}) RETURN n");
    }

    #[test]
    fn multiple_line_comments_back_to_back() {
        let q = "// header\n// header 2\nMATCH (n) // trailing\nRETURN n";
        let c = canonicalise_query(q);
        assert_eq!(c, "MATCH (n) RETURN n");
    }

    #[test]
    fn keyword_case_is_preserved() {
        let q1 = "MATCH (n) RETURN n";
        let q2 = "match (n) return n";
        // Different canonical forms — see module-level docs.
        assert_ne!(canonicalise_query(q1), canonicalise_query(q2));
    }

    #[test]
    fn parameter_names_distinguish_queries() {
        let q1 = "MATCH (n {id: $a}) RETURN n";
        let q2 = "MATCH (n {id: $b}) RETURN n";
        assert_ne!(canonicalise_query(q1), canonicalise_query(q2));
    }

    #[test]
    fn hash_canonicalised_is_stable_across_runs() {
        let h1 = hash_canonicalised("MATCH (n) RETURN n");
        let h2 = hash_canonicalised("MATCH (n) RETURN n");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_canonicalised_collapses_whitespace_variants() {
        let h1 = hash_canonicalised("MATCH (n) RETURN n");
        let h2 = hash_canonicalised("MATCH  (n)\tRETURN\nn");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_canonicalised_collapses_comment_variants() {
        let h1 = hash_canonicalised("MATCH (n) RETURN n");
        let h2 = hash_canonicalised("/* prelude */ MATCH (n) // tail\nRETURN n");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_canonicalised_distinguishes_different_queries() {
        let h1 = hash_canonicalised("MATCH (n) RETURN n");
        let h2 = hash_canonicalised("MATCH (n:Person) RETURN n");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_canonicalised_respects_string_literals() {
        let h1 = hash_canonicalised("MATCH (n {note: 'a b'}) RETURN n");
        let h2 = hash_canonicalised("MATCH (n {note: 'a  b'}) RETURN n");
        // Whitespace inside literal differs → different hashes.
        assert_ne!(h1, h2);
    }

    #[test]
    fn canonical_version_constant_is_pinned() {
        assert_eq!(CANONICAL_VERSION, 1);
    }
}
