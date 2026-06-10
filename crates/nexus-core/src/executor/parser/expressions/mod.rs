//! Expression parsing: precedence climbing from OR down through comparison,
//! arithmetic, unary, and primary. Also hosts property access,
//! function-call, list/map/parenthesis forms, and the `try_parse_not_pattern`
//! fallback for negated patterns inside WHERE.
//!
//! Sub-modules:
//! - `precedence` — OR/AND/NOT/comparison/arithmetic precedence chain + operators.
//! - `primary`    — literal and simple-expression parsers.
//! - `identifier` — identifier-expression dispatch (function calls, property access,
//!                  label predicates, map projections, array indexing).
//! - `literals`   — list/point literal parsers.
//! - `structured` — map projection, CASE, EXISTS, COLLECT { }, and pattern helpers.

mod identifier;
mod literals;
mod precedence;
mod primary;
mod structured;

use super::CypherParser;
use super::ast::*;
use crate::Result;

impl CypherParser {
    /// Parse expression (entry point — precedence climbing starts here).
    pub fn parse_expression(&mut self) -> Result<Expression> {
        self.skip_whitespace();
        self.parse_or_expression()
    }
}
