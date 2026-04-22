//! Cypher parser entry point.
//!
//! - `ast` — Abstract Syntax Tree node definitions.
//! - `clauses` — top-level parse dispatch and every `parse_*_clause`.
//! - `expressions` — expression precedence climbing, property access,
//!   function calls, list/map/parenthesis forms.
//! - `tokens` — lexer helpers: keyword/identifier/number parsing,
//!   character lookahead, whitespace skip.
//! - `tests` — test harness (cfg(test) only).

pub mod ast;
pub mod clauses;
pub mod expressions;
pub mod tokens;

#[cfg(test)]
mod tests;

pub use ast::*;

/// Cypher parser state machine. Constructors and core methods live here;
/// the `impl CypherParser` blocks that hold actual parsing logic are in
/// the sibling modules above.
pub struct CypherParser {
    /// Current position in input
    pos: usize,
    /// Input string
    input: String,
    /// Current line number
    line: usize,
    /// Current column number
    column: usize,
}
