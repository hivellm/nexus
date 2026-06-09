//! Write-side clause parsers: CREATE, MERGE, SET, DELETE, REMOVE.

use super::super::CypherParser;
use super::super::ast::*;
use super::extract_underscore_id_from_pattern;
use crate::{Error, Result};

impl CypherParser {
    /// Parse CREATE clause
    pub(super) fn parse_create_clause(&mut self) -> Result<CreateClause> {
        self.skip_whitespace();
        let mut pattern = self.parse_pattern()?;

        let external_id_expr = extract_underscore_id_from_pattern(&mut pattern)?;

        self.skip_whitespace();
        let conflict_policy = if self.peek_keyword("ON") && self.peek_keyword_at(1, "CONFLICT") {
            self.parse_keyword()?;
            self.skip_whitespace();
            self.parse_keyword()?;
            self.skip_whitespace();
            let policy_kw = self.parse_keyword()?;
            match policy_kw.to_ascii_uppercase().as_str() {
                "ERROR" => AstConflictPolicy::Error,
                "MATCH" => AstConflictPolicy::Match,
                "REPLACE" => AstConflictPolicy::Replace,
                other => {
                    return Err(self.error(&format!(
                        "ON CONFLICT must be followed by ERROR, MATCH, or REPLACE; got `{}`",
                        other
                    )));
                }
            }
        } else {
            AstConflictPolicy::Error
        };

        Ok(CreateClause {
            pattern,
            external_id_expr,
            conflict_policy,
        })
    }

    /// Parse MERGE clause
    pub(super) fn parse_merge_clause(&mut self) -> Result<MergeClause> {
        self.skip_whitespace();
        let pattern = self.parse_pattern()?;

        // Check for ON CREATE clause
        let on_create = if self.peek_keyword("ON") && self.peek_keyword_at(1, "CREATE") {
            self.skip_whitespace();
            self.parse_keyword()?; // "ON"
            self.skip_whitespace();
            self.parse_keyword()?; // "CREATE"
            self.skip_whitespace();
            // Parse SET keyword before parsing SET clause
            if self.peek_keyword("SET") {
                self.parse_keyword()?; // "SET"
                Some(self.parse_set_clause()?)
            } else {
                None
            }
        } else {
            None
        };

        // Check for ON MATCH clause
        let on_match = if self.peek_keyword("ON") && self.peek_keyword_at(1, "MATCH") {
            self.skip_whitespace();
            self.parse_keyword()?; // "ON"
            self.skip_whitespace();
            self.parse_keyword()?; // "MATCH"
            self.skip_whitespace();
            // Parse SET keyword before parsing SET clause
            if self.peek_keyword("SET") {
                self.parse_keyword()?; // "SET"
                Some(self.parse_set_clause()?)
            } else {
                None
            }
        } else {
            None
        };

        let mut pattern = pattern;
        let external_id_expr = extract_underscore_id_from_pattern(&mut pattern)?;

        Ok(MergeClause {
            pattern,
            on_create,
            on_match,
            external_id_expr,
        })
    }

    /// Parse SET clause
    pub(super) fn parse_set_clause(&mut self) -> Result<SetClause> {
        self.skip_whitespace();
        let mut items = Vec::new();

        loop {
            // Parse identifier (variable name)
            let target = self.parse_identifier()?;
            self.skip_whitespace();

            // Check if we have a property assignment (node.property = value)
            if self.peek_char() == Some('.') {
                self.consume_char();
                let property = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_char('=')?;
                self.skip_whitespace();
                let value = self.parse_expression()?;
                items.push(SetItem::Property {
                    target,
                    property,
                    value,
                });
            } else if self.peek_char() == Some(':') {
                // Label addition (node:Label) — accepts `:$param` for
                // write-side dynamic labels (advanced-types §2).
                // Chained labels on a single SET item (`SET n:A:B`) push
                // one `SetItem::Label` per segment so the engine can
                // resolve and apply them one-at-a-time, mirroring the
                // READ path and keeping error localisation sharp.
                let mut any = false;
                while self.peek_char() == Some(':') {
                    self.consume_char();
                    let label = if self.peek_char() == Some('$') {
                        self.consume_char();
                        format!("${}", self.parse_identifier()?)
                    } else {
                        self.parse_identifier()?
                    };
                    items.push(SetItem::Label {
                        target: target.clone(),
                        label,
                    });
                    self.skip_whitespace();
                    any = true;
                }
                if !any {
                    return Err(Error::storage(
                        "SET clause: expected label after ':'".to_string(),
                    ));
                }
            } else if self.peek_char() == Some('+') && self.peek_char_at(1) == Some('=') {
                // phase6_opencypher-quickwins §6 — `SET lhs += mapExpr`
                // merge semantics. Distinct from `SET lhs = mapExpr`
                // which replaces the entire bag.
                self.consume_char(); // '+'
                self.consume_char(); // '='
                self.skip_whitespace();
                let map = self.parse_expression()?;
                items.push(SetItem::MapMerge { target, map });
            } else {
                return Err(Error::storage(
                    "SET clause: expected property assignment or label".to_string(),
                ));
            }

            // Check for more items
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        Ok(SetClause { items })
    }

    /// Parse DELETE clause
    pub(super) fn parse_delete_clause(&mut self) -> Result<DeleteClause> {
        self.skip_whitespace();

        // Check for DETACH keyword
        let detach = if self.peek_keyword("DETACH") {
            self.parse_keyword()?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        // Parse list of variables to delete
        let mut items = Vec::new();

        loop {
            let variable = self.parse_identifier()?;
            items.push(variable);

            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        Ok(DeleteClause { items, detach })
    }

    /// Parse REMOVE clause
    pub(super) fn parse_remove_clause(&mut self) -> Result<RemoveClause> {
        self.skip_whitespace();
        let mut items = Vec::new();

        loop {
            // Parse identifier (variable name)
            let target = self.parse_identifier()?;
            self.skip_whitespace();

            // Check if we have a property removal (node.property)
            if self.peek_char() == Some('.') {
                self.consume_char();
                let property = self.parse_identifier()?;
                items.push(RemoveItem::Property { target, property });
            } else if self.peek_char() == Some(':') {
                // Label removal (node:Label) — accepts `:$param` for
                // write-side dynamic labels (advanced-types §2). Chained
                // labels on a single REMOVE item (`REMOVE n:A:B`) push
                // one `RemoveItem::Label` per segment.
                let mut any = false;
                while self.peek_char() == Some(':') {
                    self.consume_char();
                    let label = if self.peek_char() == Some('$') {
                        self.consume_char();
                        format!("${}", self.parse_identifier()?)
                    } else {
                        self.parse_identifier()?
                    };
                    items.push(RemoveItem::Label {
                        target: target.clone(),
                        label,
                    });
                    self.skip_whitespace();
                    any = true;
                }
                if !any {
                    return Err(Error::storage(
                        "REMOVE clause: expected label after ':'".to_string(),
                    ));
                }
            } else {
                return Err(Error::storage(
                    "REMOVE clause: expected property or label removal".to_string(),
                ));
            }

            // Check for more items
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        Ok(RemoveClause { items })
    }
}
