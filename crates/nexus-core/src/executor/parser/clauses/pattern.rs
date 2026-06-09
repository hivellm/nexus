//! Pattern parsing: nodes, relationships, labels, types, property maps,
//! quantifiers, and QPP (quantified path patterns).

use super::super::CypherParser;
use super::super::ast::*;
use crate::{Error, Result};
use std::collections::HashMap;

impl CypherParser {
    /// Parse pattern
    // Visibility elevated to `pub(in super::super)` (= `parser` level) because
    // `expressions.rs` calls this method directly. Original was `pub(super)` in
    // `clauses.rs` where `super` == `parser`; now that the code lives one level
    // deeper, the equivalent is `pub(in super::super)`.
    pub(in super::super) fn parse_pattern(&mut self) -> Result<Pattern> {
        let mut elements = Vec::new();

        // Parse first node
        let node = self.parse_node_pattern()?;
        elements.push(PatternElement::Node(node));

        // Parse relationships and nodes, or comma-separated nodes
        while self.pos < self.input.len() {
            // Check if there's a relationship pattern by looking ahead
            let saved_pos = self.pos;
            let saved_line = self.line;
            let saved_column = self.column;

            // Skip whitespace
            self.skip_whitespace();

            // Check for comma (multiple independent node patterns)
            if self.peek_char() == Some(',') {
                self.consume_char(); // consume ','
                self.skip_whitespace();

                // Parse next node pattern as independent node
                let node = self.parse_node_pattern()?;
                elements.push(PatternElement::Node(node));
                continue;
            }

            // Check if we have a relationship pattern
            if self.peek_char() == Some('-')
                || self.peek_char() == Some('<')
                || self.peek_char() == Some('>')
            {
                // Parse relationship
                let rel = self.parse_relationship_pattern()?;
                elements.push(PatternElement::Relationship(rel));

                // Parse next node
                let node = self.parse_node_pattern()?;
                elements.push(PatternElement::Node(node));
                continue;
            }

            // QPP: `( subPattern ) quantifier` — Cypher 25 / GQL.
            // Triggered by a parenthesis directly after a node, with
            // no intervening `-` / `<` / `>` rel-operator. The closing
            // paren must be followed by a quantifier token (`{m,n}`,
            // `*`, `+`, `?`). Anything else is not a QPP — we restore
            // position and let the caller terminate the pattern.
            //
            // An optional path-mode keyword (`WALK | TRAIL | ACYCLIC
            // | SIMPLE`) may precede the opening paren: e.g.
            // `(a)TRAIL ((x)-[r]->(y)){2,5}(b)`. The keyword is
            // consumed only when a QPP group actually follows; if
            // the lookahead does not form a QPP the keyword
            // characters are restored alongside the rest of the
            // backtrack.
            let mode_save_pos = self.pos;
            let mode_save_line = self.line;
            let mode_save_column = self.column;
            let parsed_mode = self.try_parse_qpp_mode_keyword()?;
            self.skip_whitespace();
            if self.peek_char() == Some('(') {
                match self.try_parse_qpp_group()? {
                    Some(mut group) => {
                        if let Some(m) = parsed_mode {
                            group.mode = m;
                            group.mode_explicit = true;
                        }
                        // Slice-1 QPP normalisation
                        // (`phase6_opencypher-quantified-path-patterns`):
                        // when the group is the textbook
                        // `( ()-[:T]->() ){m,n}` shape, push it as a
                        // plain quantified Relationship so every
                        // downstream consumer (planner, projection,
                        // EXISTS subqueries, …) treats it exactly
                        // like a legacy `*m..n` form. Groups that
                        // carry inner state survive as
                        // QuantifiedGroup and the planner surfaces
                        // a clean ERR_QPP_NOT_IMPLEMENTED for them.
                        if let Some(rel) = group.try_lower_to_var_length_rel() {
                            elements.push(PatternElement::Relationship(rel));
                        } else {
                            elements.push(PatternElement::QuantifiedGroup(group));
                        }
                        // The textbook QPP shape `(a)( body ){m,n}(b)`
                        // is followed by a trailing boundary node.
                        // Without parsing it here the outer pattern
                        // ends at the group and `(b)` gets dropped,
                        // which leaves the planner without a target
                        // variable and silently breaks projections
                        // (`RETURN b` returns whatever happened to
                        // be in the last expand slot).
                        self.skip_whitespace();
                        if self.peek_char() == Some('(') {
                            let node = self.parse_node_pattern()?;
                            elements.push(PatternElement::Node(node));
                        }
                        continue;
                    }
                    None => {
                        // The mode keyword (if any) was consumed
                        // optimistically; the lookahead did not
                        // form a real QPP, so unwind to before the
                        // keyword and let the outer loop terminate
                        // the pattern normally.
                        self.pos = saved_pos;
                        self.line = saved_line;
                        self.column = saved_column;
                        break;
                    }
                }
            }
            // No QPP group at this position — restore mode-keyword
            // consumption so the outer pattern parser sees the
            // tokens it expects (a `WALK` / `TRAIL` / `ACYCLIC` /
            // `SIMPLE` here was a false positive).
            if parsed_mode.is_some() {
                self.pos = mode_save_pos;
                self.line = mode_save_line;
                self.column = mode_save_column;
            }

            // Restore position if no relationship, comma, or QPP found
            self.pos = saved_pos;
            self.line = saved_line;
            self.column = saved_column;
            break;
        }

        Ok(Pattern {
            elements,
            path_variable: None, // Set by caller if path variable assignment detected
        })
    }

    /// Attempt to parse a quantified path pattern group starting at
    /// the current position. Returns `Ok(None)` when the lookahead
    /// does not form a valid QPP (caller should backtrack); returns
    /// `Ok(Some(group))` on success. Rejects nested QPP (one level
    /// deep — Cypher 25 restriction) and empty bodies.
    fn try_parse_qpp_group(&mut self) -> Result<Option<QuantifiedGroup>> {
        debug_assert_eq!(self.peek_char(), Some('('));
        let restore_pos = self.pos;
        let restore_line = self.line;
        let restore_column = self.column;

        self.consume_char(); // '('
        self.skip_whitespace();

        // Body must start with a node pattern. Anything else fails
        // the QPP match and the caller backtracks.
        if self.peek_char() != Some('(') {
            self.pos = restore_pos;
            self.line = restore_line;
            self.column = restore_column;
            return Ok(None);
        }

        // Detect nested QPP before entering the recursive body parser:
        // `( ( ( ... ) ){..} ... )` starts `(((`. Since QPP's recursive
        // descent cannot parse a body whose first element is itself a
        // QPP, we intercept the shape explicitly and surface the
        // Cypher 25 restriction with a clean error.
        {
            let mut probe = self.pos + 1; // skip the inner `(`
            while probe < self.input.len() {
                let c = self.input.as_bytes()[probe] as char;
                if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
                    probe += 1;
                } else {
                    break;
                }
            }
            if probe < self.input.len() && self.input.as_bytes()[probe] as char == '(' {
                return Err(Error::CypherSyntax(
                    "ERR_QPP_NESTING_TOO_DEEP: quantified path patterns \
                     cannot nest (Cypher 25 restriction)"
                        .to_string(),
                ));
            }
        }

        let inner = match self.parse_pattern() {
            Ok(pattern) => pattern,
            Err(_) => {
                self.pos = restore_pos;
                self.line = restore_line;
                self.column = restore_column;
                return Ok(None);
            }
        };

        // Reject nested QPP (one level deep — Cypher 25).
        if inner
            .elements
            .iter()
            .any(|e| matches!(e, PatternElement::QuantifiedGroup(_)))
        {
            return Err(Error::CypherSyntax(
                "ERR_QPP_NESTING_TOO_DEEP: quantified path patterns \
                 cannot nest (Cypher 25 restriction)"
                    .to_string(),
            ));
        }

        self.skip_whitespace();

        // Optional inner `WHERE` clause inside the QPP body
        // (Cypher 25 §1.4): `( body WHERE predicate ){m,n}`. The
        // predicate gets evaluated against the per-iteration
        // bindings, so an iteration that fails it is dropped
        // before the row is emitted. We only consume `WHERE` if
        // it sits right before the closing `)` — anything else
        // means the body itself never closed and we should
        // backtrack so the outer pattern terminates normally.
        let where_clause = if self.peek_keyword("WHERE") {
            self.parse_keyword()?;
            self.skip_whitespace();
            let expr = self.parse_expression()?;
            self.skip_whitespace();
            Some(expr)
        } else {
            None
        };

        if self.peek_char() != Some(')') {
            self.pos = restore_pos;
            self.line = restore_line;
            self.column = restore_column;
            return Ok(None);
        }
        self.consume_char(); // ')'

        // Quantifier is mandatory. A bare `( subPattern )` without a
        // quantifier is not a QPP — backtrack so the caller can
        // terminate the outer pattern normally.
        let quantifier = match self.parse_relationship_quantifier()? {
            Some(q) => q,
            None => {
                self.pos = restore_pos;
                self.line = restore_line;
                self.column = restore_column;
                return Ok(None);
            }
        };

        // Reject `{n,m}` where n > m.
        if let RelationshipQuantifier::Range(lo, hi) = &quantifier {
            if lo > hi {
                return Err(Error::CypherSyntax(format!(
                    "ERR_QPP_INVALID_QUANTIFIER: lower bound {lo} \
                     exceeds upper bound {hi}"
                )));
            }
        }

        Ok(Some(QuantifiedGroup {
            inner: inner.elements,
            quantifier,
            where_clause,
            mode: crate::executor::types::QppMode::default(),
            mode_explicit: false,
        }))
    }

    /// Optionally consume a path-mode keyword (`WALK | TRAIL |
    /// ACYCLIC | SIMPLE`) at the current position. Returns `Some`
    /// when one was consumed (caller should pair it with a QPP
    /// group), `None` otherwise. The function does not validate
    /// that a QPP follows — the caller decides whether to honour
    /// the keyword or restore the pre-keyword position.
    fn try_parse_qpp_mode_keyword(&mut self) -> Result<Option<crate::executor::types::QppMode>> {
        use crate::executor::types::QppMode;
        let save_pos = self.pos;
        let save_line = self.line;
        let save_column = self.column;
        for (kw, mode) in [
            ("WALK", QppMode::Walk),
            ("TRAIL", QppMode::Trail),
            ("ACYCLIC", QppMode::Acyclic),
            ("SIMPLE", QppMode::Simple),
        ] {
            if self.peek_keyword(kw) {
                self.parse_keyword()?;
                // Demand whitespace after the keyword so identifiers
                // that start with one of these letters (e.g. a node
                // variable named `Walking`) do not falsely match.
                let after = self.peek_char();
                if after
                    .map(|c| c == ' ' || c == '\t' || c == '(')
                    .unwrap_or(false)
                {
                    return Ok(Some(mode));
                }
                // Identifier-like continuation — not the keyword we
                // wanted. Restore and signal absence.
                self.pos = save_pos;
                self.line = save_line;
                self.column = save_column;
                return Ok(None);
            }
        }
        Ok(None)
    }

    /// Parse node pattern
    // Visibility elevated to `pub(in super::super)` — called from `expressions.rs`.
    pub(in super::super) fn parse_node_pattern(&mut self) -> Result<NodePattern> {
        self.expect_char('(')?;
        self.skip_whitespace();

        let variable = if self.is_identifier_start() {
            Some(self.parse_identifier()?)
        } else {
            None
        };

        self.skip_whitespace();
        let labels = if self.peek_char() == Some(':') {
            self.parse_labels()?
        } else {
            Vec::new()
        };

        self.skip_whitespace();
        let properties = if self.peek_char() == Some('{') {
            Some(self.parse_property_map()?)
        } else {
            None
        };

        self.skip_whitespace();
        self.expect_char(')')?;

        // phase9_external-node-ids §4.6 — `external_id_expr` is populated
        // by the clause-level extractor for CREATE / MERGE patterns.
        // MATCH inline `{_id: …}` form is not yet routed through the
        // external-id index seek; the property map keeps `_id` so the
        // regular filter pipeline handles it (current behaviour).
        Ok(NodePattern {
            variable,
            labels,
            properties,
            external_id_expr: None,
        })
    }

    /// Parse relationship pattern
    // Visibility elevated to `pub(in super::super)` — called from `expressions.rs`.
    pub(in super::super) fn parse_relationship_pattern(&mut self) -> Result<RelationshipPattern> {
        // Parse initial direction: "-" or "<-"
        let left_arrow = if self.peek_char() == Some('<') {
            self.consume_char();
            self.expect_char('-')?;
            true
        } else if self.peek_char() == Some('-') {
            self.consume_char();
            false
        } else {
            return Err(Error::CypherSyntax(format!(
                "Expected relationship direction at line 1, column {}",
                self.pos + 1
            )));
        };

        self.expect_char('[')?;
        self.skip_whitespace();

        let variable = if self.is_identifier_start() {
            Some(self.parse_identifier()?)
        } else {
            None
        };

        self.skip_whitespace();
        let types = if self.peek_char() == Some(':') {
            self.parse_types()?
        } else {
            Vec::new()
        };

        self.skip_whitespace();

        // Check if next token is a quantifier (starts with *, +, ?, or { followed by digit/comma/})
        // or a property map (starts with { followed by identifier)
        let (properties, quantifier) = if self.peek_char() == Some('{') {
            // Peek ahead to see if it's a quantifier or property map
            // Check character after '{' (skip whitespace)
            let mut peek_offset = 1;
            let mut is_quantifier = false;
            while peek_offset < self.input.len() - self.pos {
                if let Some(c) = self.peek_char_at(peek_offset) {
                    if c.is_whitespace() {
                        peek_offset += 1;
                        continue;
                    }
                    // If next char is digit, comma, or '}', it's a quantifier
                    is_quantifier = c.is_ascii_digit() || c == ',' || c == '}';
                    break;
                } else {
                    break;
                }
            }

            if is_quantifier {
                // It's a quantifier, not properties
                (None, self.parse_relationship_quantifier()?)
            } else {
                // It's a property map
                (
                    Some(self.parse_property_map()?),
                    self.parse_relationship_quantifier()?,
                )
            }
        } else {
            // No properties, check for quantifier
            (None, self.parse_relationship_quantifier()?)
        };

        self.skip_whitespace();
        self.expect_char(']')?;

        // Parse final direction: "->" or "-"
        self.expect_char('-')?;
        let right_arrow = if self.peek_char() == Some('>') {
            self.consume_char();
            true
        } else {
            false
        };

        // Determine final direction
        let direction = match (left_arrow, right_arrow) {
            (true, false) => RelationshipDirection::Incoming, // <-[r]-
            (false, true) => RelationshipDirection::Outgoing, // -[r]->
            (false, false) => RelationshipDirection::Both,    // -[r]-
            (true, true) => {
                return Err(Error::CypherSyntax(format!(
                    "Invalid relationship direction <-[]-> at line 1, column {}",
                    self.pos + 1
                )));
            }
        };

        Ok(RelationshipPattern {
            variable,
            types,
            direction,
            properties,
            quantifier,
        })
    }

    /// Parse relationship direction
    // Visibility elevated to `pub(in super::super)` (= `parser` level): called
    // from `parser/tests.rs`, which sat next to this method before the split.
    pub(in super::super) fn parse_relationship_direction(
        &mut self,
    ) -> Result<RelationshipDirection> {
        match self.peek_char() {
            Some('-') => {
                self.consume_char();
                if self.peek_char() == Some('>') {
                    self.consume_char();
                    Ok(RelationshipDirection::Outgoing)
                } else {
                    Ok(RelationshipDirection::Both)
                }
            }
            Some('<') => {
                self.consume_char();
                if self.peek_char() == Some('-') {
                    self.consume_char();
                    Ok(RelationshipDirection::Incoming)
                } else {
                    Err(self.error("Invalid relationship direction"))
                }
            }
            _ => Err(self.error("Expected relationship direction")),
        }
    }

    /// Parse labels.
    ///
    /// phase6_opencypher-advanced-types §2 — parameter-valued labels.
    /// A `:$param` label is encoded as the sentinel string `"$param"`
    /// (leading `$` is never a valid identifier character, so downstream
    /// writers can unambiguously recognise and resolve it against the
    /// execution-time parameter map via
    /// [`crate::engine::dynamic_labels::resolve_labels`]).
    pub(super) fn parse_labels(&mut self) -> Result<Vec<String>> {
        let mut labels = Vec::new();

        while self.peek_char() == Some(':') {
            self.consume_char(); // consume ':'
            if self.peek_char() == Some('$') {
                self.consume_char(); // consume '$'
                let param = self.parse_identifier()?;
                labels.push(format!("${param}"));
            } else {
                let label = self.parse_identifier()?;
                labels.push(label);
            }
        }

        Ok(labels)
    }

    /// Parse types
    pub(super) fn parse_types(&mut self) -> Result<Vec<String>> {
        let mut types = Vec::new();

        // First type must be preceded by ':'
        if self.peek_char() == Some(':') {
            self.consume_char(); // consume ':'
            let r#type = self.parse_identifier()?;
            types.push(r#type);

            // Additional types can be separated by '|' (e.g., :TYPE1|TYPE2)
            self.skip_whitespace();
            while self.peek_char() == Some('|') {
                self.consume_char(); // consume '|'
                self.skip_whitespace();
                let r#type = self.parse_identifier()?;
                types.push(r#type);
                self.skip_whitespace();
            }
        }

        Ok(types)
    }

    /// Parse property map
    pub(in super::super) fn parse_property_map(&mut self) -> Result<PropertyMap> {
        self.expect_char('{')?;
        self.skip_whitespace();

        let mut properties = HashMap::new();

        while self.peek_char() != Some('}') {
            let key = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_char(':')?;
            self.skip_whitespace();
            let value = self.parse_expression()?;
            properties.insert(key, value);

            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            }
        }

        self.expect_char('}')?;

        Ok(PropertyMap { properties })
    }

    /// Parse relationship quantifier
    pub(super) fn parse_relationship_quantifier(
        &mut self,
    ) -> Result<Option<RelationshipQuantifier>> {
        match self.peek_char() {
            Some('*') => {
                self.consume_char();
                // Check if there's a number after * (e.g., *1..3 or *5)
                // Skip whitespace first
                self.skip_whitespace();
                if self.is_digit() {
                    // Parse range quantifier without braces: *1..3 or *5
                    self.parse_range_quantifier_without_braces()
                } else {
                    // Just * means zero or more
                    Ok(Some(RelationshipQuantifier::ZeroOrMore))
                }
            }
            Some('+') => {
                self.consume_char();
                Ok(Some(RelationshipQuantifier::OneOrMore))
            }
            Some('?') => {
                self.consume_char();
                Ok(Some(RelationshipQuantifier::ZeroOrOne))
            }
            Some('{') => self.parse_range_quantifier(),
            _ => Ok(None),
        }
    }

    /// Parse range quantifier without braces: *1..3 or *5
    pub(super) fn parse_range_quantifier_without_braces(
        &mut self,
    ) -> Result<Option<RelationshipQuantifier>> {
        let start = if self.is_digit() {
            Some(self.parse_number()?)
        } else {
            None
        };

        // Check for range separator: ',' or '..'
        if self.peek_char() == Some(',')
            || (self.peek_char() == Some('.') && self.peek_char_at(1) == Some('.'))
        {
            if self.peek_char() == Some(',') {
                self.consume_char();
            } else {
                // Consume '..'
                self.consume_char();
                self.consume_char();
            }
            let end = if self.is_digit() {
                Some(self.parse_number()?)
            } else {
                None
            };

            match (start, end) {
                (Some(n), Some(m)) => {
                    Ok(Some(RelationshipQuantifier::Range(n as usize, m as usize)))
                }
                (Some(n), None) => Ok(Some(RelationshipQuantifier::Range(n as usize, usize::MAX))),
                (None, Some(m)) => Ok(Some(RelationshipQuantifier::Range(0, m as usize))),
                (None, None) => Ok(Some(RelationshipQuantifier::ZeroOrMore)),
            }
        } else {
            // No range separator, just a number means exact count
            if let Some(n) = start {
                Ok(Some(RelationshipQuantifier::Exact(n as usize)))
            } else {
                Ok(Some(RelationshipQuantifier::ZeroOrMore))
            }
        }
    }

    /// Parse range quantifier
    pub(super) fn parse_range_quantifier(&mut self) -> Result<Option<RelationshipQuantifier>> {
        self.expect_char('{')?;

        let start = if self.is_digit() {
            Some(self.parse_number()?)
        } else {
            None
        };

        // Check for range separator: ',' or '..'
        if self.peek_char() == Some(',')
            || (self.peek_char() == Some('.') && self.peek_char_at(1) == Some('.'))
        {
            if self.peek_char() == Some(',') {
                self.consume_char();
            } else {
                // Consume '..'
                self.consume_char();
                self.consume_char();
            }
            let end = if self.is_digit() {
                Some(self.parse_number()?)
            } else {
                None
            };

            self.expect_char('}')?;

            match (start, end) {
                (Some(n), Some(m)) => {
                    Ok(Some(RelationshipQuantifier::Range(n as usize, m as usize)))
                }
                (Some(n), None) => Ok(Some(RelationshipQuantifier::Range(n as usize, usize::MAX))),
                (None, Some(m)) => Ok(Some(RelationshipQuantifier::Range(0, m as usize))),
                (None, None) => Ok(Some(RelationshipQuantifier::ZeroOrMore)),
            }
        } else {
            self.expect_char('}')?;

            if let Some(n) = start {
                Ok(Some(RelationshipQuantifier::Exact(n as usize)))
            } else {
                Ok(Some(RelationshipQuantifier::ZeroOrMore))
            }
        }
    }
}
