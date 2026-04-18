//! Lexer/token helpers of the recursive-descent parser: identifier,
//! keyword, and number parsers; character lookahead (`peek_char`,
//! `peek_char_at`, `consume_char`, `expect_char`); whitespace/comment
//! skip; and the `is_*` predicates that drive clause boundary detection.

use super::CypherParser;
use super::ast::UnaryOperator;
use crate::{Error, Result};

impl CypherParser {
    /// Parse unary operator
    pub(super) fn parse_unary_operator(&mut self) -> Option<UnaryOperator> {
        match self.peek_char() {
            Some('+') => {
                self.consume_char();
                Some(UnaryOperator::Plus)
            }
            Some('-') => {
                self.consume_char();
                Some(UnaryOperator::Minus)
            }
            _ => None,
        }
    }

    /// Parse keyword
    pub(super) fn parse_keyword(&mut self) -> Result<String> {
        self.skip_whitespace(); // Skip whitespace before parsing keyword

        let start = self.pos;

        while self.pos < self.input.len() && self.is_keyword_char() {
            self.consume_char();
        }

        let keyword = self.input[start..self.pos].to_string();
        self.skip_whitespace(); // Skip whitespace after parsing keyword
        Ok(keyword)
    }

    /// Parse identifier
    pub(super) fn parse_identifier(&mut self) -> Result<String> {
        let start = self.pos;

        if !self.is_identifier_start() {
            return Err(self.error("Expected identifier"));
        }

        self.consume_char();

        while self.pos < self.input.len() && self.is_identifier_char() {
            self.consume_char();
        }

        Ok(self.input[start..self.pos].to_string())
    }

    /// Parse number
    pub(super) fn parse_number(&mut self) -> Result<i64> {
        let start = self.pos;

        while self.pos < self.input.len() && self.is_digit() {
            self.consume_char();
        }

        self.input[start..self.pos]
            .parse::<i64>()
            .map_err(|_| self.error("Invalid number"))
    }

    /// Check if character is keyword character
    pub(super) fn is_keyword_char(&self) -> bool {
        self.peek_char()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
    }

    /// Check if we're at a clause boundary
    pub(super) fn is_clause_boundary(&self) -> bool {
        // Check if we're at the start of a valid clause keyword
        self.peek_keyword("OPTIONAL") // Check for OPTIONAL MATCH first
            || self.peek_keyword("MATCH")
            || self.peek_keyword("CREATE")
            || self.peek_keyword("MERGE")
            || self.peek_keyword("SET")
            || self.peek_keyword("DELETE")
            || self.peek_keyword("DETACH")  // For DETACH DELETE
            || self.peek_keyword("REMOVE")
            || self.peek_keyword("WITH")
            || self.peek_keyword("UNWIND")
            || self.peek_keyword("UNION")
            || self.peek_keyword("WHERE")
            || self.peek_keyword("RETURN")
            || self.peek_keyword("ORDER")
            || self.peek_keyword("LIMIT")
            || self.peek_keyword("SKIP")
            || self.peek_keyword("SHOW")  // For SHOW DATABASES/USERS
            || self.peek_keyword("DROP")  // For DROP DATABASE/INDEX/CONSTRAINT
            || self.peek_keyword("BEGIN")  // For BEGIN TRANSACTION
            || self.peek_keyword("COMMIT")  // For COMMIT TRANSACTION
            || self.peek_keyword("ROLLBACK")  // For ROLLBACK TRANSACTION
            || self.peek_keyword("GRANT")  // For GRANT permissions
            || self.peek_keyword("REVOKE") // For REVOKE permissions
            || self.peek_keyword("CALL")  // For CALL procedures and subqueries
            || self.peek_keyword("USE")  // For USE DATABASE
            || self.peek_keyword("LOAD")  // For LOAD CSV
            || self.peek_keyword("FOREACH") // For FOREACH clause
            || self.peek_keyword("ALTER") // For ALTER DATABASE
            || self.peek_keyword("TERMINATE") // For TERMINATE QUERY
    }

    /// Check if character is identifier start
    pub(super) fn is_identifier_start(&self) -> bool {
        self.peek_char()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
    }

    /// Check if character is identifier character
    pub(super) fn is_identifier_char(&self) -> bool {
        self.peek_char()
            .map(|c| c.is_ascii_alphanumeric() || c == '_')
            .unwrap_or(false)
    }

    /// Check if character is digit
    pub(super) fn is_digit(&self) -> bool {
        self.peek_char()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
    }

    /// Peek at current character
    pub(super) fn peek_char(&self) -> Option<char> {
        self.input.chars().nth(self.pos)
    }

    /// Consume current character
    pub(super) fn consume_char(&mut self) -> Option<char> {
        if self.pos < self.input.len() {
            let ch = self.input.chars().nth(self.pos).unwrap();
            self.pos += 1;

            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }

            Some(ch)
        } else {
            None
        }
    }

    /// Expect specific character
    pub(super) fn expect_char(&mut self, expected: char) -> Result<()> {
        if self.consume_char() == Some(expected) {
            Ok(())
        } else {
            Err(self.error(&format!("Expected '{}'", expected)))
        }
    }

    /// Expect specific keyword
    pub(super) fn expect_keyword(&mut self, expected: &str) -> Result<()> {
        let keyword = self.parse_keyword()?;
        if keyword.to_uppercase() == expected.to_uppercase() {
            Ok(())
        } else {
            Err(self.error(&format!("Expected keyword '{}'", expected)))
        }
    }

    /// Check if next token is keyword
    pub(super) fn peek_keyword_at(&self, offset: usize, keyword: &str) -> bool {
        let start = self.pos;
        let mut pos = start;

        // Skip first n keywords (offset)
        for _ in 0..offset {
            // Skip whitespace
            while pos < self.input.len() {
                let ch = self.input.chars().nth(pos).unwrap();
                if ch.is_whitespace() {
                    pos += 1;
                } else {
                    break;
                }
            }
            // Skip word
            while pos < self.input.len() {
                let ch = self.input.chars().nth(pos).unwrap();
                if ch.is_alphanumeric() || ch == '_' {
                    pos += 1;
                } else {
                    break;
                }
            }
        }

        // Skip whitespace before the target keyword
        while pos < self.input.len() {
            let ch = self.input.chars().nth(pos).unwrap();
            if ch.is_whitespace() {
                pos += 1;
            } else {
                break;
            }
        }

        // Check if keyword matches
        let remaining = &self.input[pos..];
        remaining
            .to_uppercase()
            .starts_with(&keyword.to_uppercase())
    }

    pub(super) fn peek_keyword(&self, keyword: &str) -> bool {
        let start = self.pos;
        let mut pos = start;

        // Skip whitespace
        while pos < self.input.len() {
            let ch = self.input.chars().nth(pos).unwrap();
            if ch.is_whitespace() {
                pos += 1;
            } else {
                break;
            }
        }

        // Check if keyword matches
        let remaining = &self.input[pos..];
        remaining
            .to_uppercase()
            .starts_with(&keyword.to_uppercase())
            && (remaining.len() == keyword.len()
                || remaining
                    .chars()
                    .nth(keyword.len())
                    .is_none_or(|c| !c.is_ascii_alphanumeric()))
    }

    /// Skip whitespace
    pub(super) fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.input.chars().nth(self.pos).unwrap();
            if ch.is_whitespace() {
                self.consume_char();
            } else {
                break;
            }
        }
    }

    /// Create error with position information
    pub(super) fn error(&self, message: &str) -> Error {
        Error::CypherSyntax(format!(
            "{} at line {}, column {}",
            message, self.line, self.column
        ))
    }
}
