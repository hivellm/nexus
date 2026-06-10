//! Structured expression parsers: map projections, CASE expressions,
//! EXISTS { … }, COLLECT { … } subqueries, and the pattern-until-brace
//! helper used by both EXISTS and pattern comprehensions.

use super::super::CypherParser;
use super::super::ast::*;
use crate::Result;

impl CypherParser {
    /// Parse map projection items: {.name, .age AS age_alias, fullName: n.name}
    pub(super) fn parse_map_projection_items(&mut self) -> Result<Vec<MapProjectionItem>> {
        self.expect_char('{')?;
        self.skip_whitespace();

        let mut items = Vec::new();

        loop {
            self.skip_whitespace();

            // Check for closing brace
            if self.peek_char() == Some('}') {
                self.consume_char();
                break;
            }

            // Check if it's a property projection (.name) or virtual key (name: expr)
            if self.peek_char() == Some('.') {
                // Property projection: .name or .name AS alias
                self.consume_char(); // consume '.'
                let property = self.parse_identifier()?;
                self.skip_whitespace();

                // Check for AS alias
                let alias = if self.peek_keyword("AS") {
                    self.expect_keyword("AS")?;
                    self.skip_whitespace();
                    Some(self.parse_identifier()?)
                } else {
                    None
                };

                items.push(MapProjectionItem::Property { property, alias });
            } else {
                // Virtual key: name: expression
                let key = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_char(':')?;
                self.skip_whitespace();
                let expression = self.parse_expression()?;

                items.push(MapProjectionItem::VirtualKey { key, expression });
            }

            self.skip_whitespace();

            // Check for comma separator
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else if self.peek_char() != Some('}') {
                return Err(self.error("Expected ',' or '}' in map projection"));
            }
        }

        Ok(items)
    }

    /// Parse case expression
    pub(super) fn parse_case_expression(&mut self) -> Result<Expression> {
        self.expect_keyword("CASE")?; // consume CASE

        let input = if self.peek_char() != Some('W') {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        let mut when_clauses = Vec::new();

        while self.peek_keyword("WHEN") {
            self.expect_keyword("WHEN")?;
            let condition = self.parse_expression()?;
            self.expect_keyword("THEN")?;
            let result = self.parse_expression()?;
            when_clauses.push(WhenClause { condition, result });
        }

        let else_clause = if self.peek_keyword("ELSE") {
            self.expect_keyword("ELSE")?;
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };

        self.expect_keyword("END")?;

        Ok(Expression::Case {
            input,
            when_clauses,
            else_clause,
        })
    }

    /// Parse `COLLECT { … }` subquery expression
    /// (phase6_opencypher-subquery-transactions §9).
    ///
    /// Reuses the same clause-parsing loop as `CALL { … }` so every
    /// MATCH / WITH / WHERE / RETURN combination accepted by the
    /// outer query is also accepted inside the collect subquery.
    /// The inner MUST terminate with a `RETURN` clause — that's the
    /// values the LIST is folded over.
    pub(super) fn parse_collect_subquery_expression(&mut self) -> Result<Expression> {
        self.expect_keyword("COLLECT")?;
        self.skip_whitespace();
        self.expect_char('{')?;
        self.skip_whitespace();

        let mut clauses = Vec::new();
        while self.pos < self.input.len() {
            self.skip_whitespace();
            if self.peek_char() == Some('}') {
                self.consume_char();
                break;
            }
            if self.is_clause_boundary() {
                let clause = self.parse_clause(clauses.last())?;
                clauses.push(clause);
            } else {
                break;
            }
        }

        if clauses.is_empty() {
            return Err(self.error(
                "ERR_COLLECT_SUBQUERY_EMPTY: COLLECT { … } must contain \
                 at least one clause",
            ));
        }

        // Cypher 25 requires the inner to end in RETURN — that's the
        // expression the LIST is built over. Reject inputs that drop
        // the RETURN so we don't silently emit an empty list.
        let last_is_return = matches!(clauses.last(), Some(Clause::Return(_)));
        if !last_is_return {
            return Err(self.error(
                "ERR_COLLECT_SUBQUERY_NO_RETURN: COLLECT { … } must \
                 terminate with a RETURN clause",
            ));
        }

        let inner = CypherQuery {
            clauses,
            params: std::collections::HashMap::new(),
            graph_scope: None,
        };
        Ok(Expression::CollectSubquery {
            inner: Box::new(inner),
        })
    }

    /// Parse EXISTS expression
    pub(super) fn parse_exists_expression(&mut self) -> Result<Expression> {
        self.expect_keyword("EXISTS")?; // consume EXISTS
        self.skip_whitespace();

        // Expect opening brace {
        self.expect_char('{')?;
        self.skip_whitespace();

        // Parse the pattern inside the braces
        // We need to stop before WHERE or closing brace
        let pattern = self.parse_pattern_until_where_or_brace()?;
        self.skip_whitespace();

        // Parse optional WHERE clause
        let where_clause = if self.peek_keyword("WHERE") {
            self.expect_keyword("WHERE")?;
            self.skip_whitespace();
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };
        self.skip_whitespace();

        // Expect closing brace }
        self.expect_char('}')?;

        Ok(Expression::Exists {
            pattern,
            where_clause,
        })
    }

    /// Parse pattern until WHERE keyword or closing brace
    /// This is used for EXISTS and Pattern Comprehensions
    pub(super) fn parse_pattern_until_where_or_brace(&mut self) -> Result<Pattern> {
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

            // Check for WHERE keyword (stop parsing pattern)
            if self.peek_keyword("WHERE") {
                self.pos = saved_pos;
                self.line = saved_line;
                self.column = saved_column;
                break;
            }

            // Check for closing brace (stop parsing pattern)
            if self.peek_char() == Some('}') {
                self.pos = saved_pos;
                self.line = saved_line;
                self.column = saved_column;
                break;
            }

            // Check for pipe (|) - used in comprehensions
            if self.peek_char() == Some('|') {
                self.pos = saved_pos;
                self.line = saved_line;
                self.column = saved_column;
                break;
            }

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
            } else {
                // Restore position if no relationship or comma found
                self.pos = saved_pos;
                self.line = saved_line;
                self.column = saved_column;
                break;
            }
        }

        Ok(Pattern {
            elements,
            path_variable: None, // Set by caller if path variable assignment detected
        })
    }
}
