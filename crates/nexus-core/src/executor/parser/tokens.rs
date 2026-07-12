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
            || self.peek_keyword("SAVEPOINT") // phase6_opencypher-advanced-types §5
            || self.peek_keyword("RELEASE") // RELEASE SAVEPOINT
            // EXPLAIN / PROFILE prefix a whole query. Without these the
            // main parse loop broke at position 0 and returned an EMPTY
            // AST for any `EXPLAIN ...` / `PROFILE ...` input — the
            // engine never saw the Explain/Profile clause and the
            // executor's planner rejected the empty re-parse.
            || self.peek_keyword("EXPLAIN")
            || self.peek_keyword("PROFILE")
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

    /// Peek at the character starting at `self.pos`.
    ///
    /// Uses `self.input[self.pos..].chars().next()` rather than
    /// `self.input.chars().nth(self.pos)`: the former is O(1) because
    /// byte-slice creation plus a single UTF-8 decode is constant-time,
    /// while the latter walks the iterator from the start of the input
    /// on every call (O(n) per peek → O(n²) over a full parse).
    /// Cypher queries are predominantly ASCII, so `pos` is a byte
    /// offset that coincides with a character boundary in practice.
    pub(super) fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    /// Consume and return the character at `self.pos`.
    pub(super) fn consume_char(&mut self) -> Option<char> {
        if self.pos < self.input.len() {
            let ch = self.input[self.pos..].chars().next()?;
            // Advance by the char's UTF-8 width, not 1 byte — otherwise a
            // multi-byte char (any non-ASCII text in a string literal,
            // property value, etc.) leaves `pos` mid-sequence and the next
            // `self.input[self.pos..]` slice panics on a non-char boundary.
            self.pos += ch.len_utf8();

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
                let ch = self.input[pos..].chars().next().unwrap();
                if ch.is_whitespace() {
                    pos += ch.len_utf8();
                } else {
                    break;
                }
            }
            // Skip word
            while pos < self.input.len() {
                let ch = self.input[pos..].chars().next().unwrap();
                if ch.is_alphanumeric() || ch == '_' {
                    pos += ch.len_utf8();
                } else {
                    break;
                }
            }
        }

        // Skip whitespace before the target keyword
        while pos < self.input.len() {
            let ch = self.input[pos..].chars().next().unwrap();
            if ch.is_whitespace() {
                pos += ch.len_utf8();
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
            let ch = self.input[pos..].chars().next().unwrap();
            if ch.is_whitespace() {
                pos += ch.len_utf8();
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

    /// Skip whitespace and Cypher comments (`// line comment` and
    /// `/* block comment */`, per the openCypher grammar). Comments are
    /// treated as insignificant exactly like whitespace, so they may
    /// appear anywhere whitespace is currently allowed — including
    /// before the very first clause of a query (a leading `//` line was
    /// previously left unskipped, so the parser saw `/` as the start of
    /// the first token, matched no clause keyword, and returned an
    /// empty `CypherQuery` that the planner then rejected with "Query
    /// must contain at least one clause").
    pub(super) fn skip_whitespace(&mut self) {
        loop {
            let mut advanced = false;

            while self.pos < self.input.len() {
                let ch = self.input[self.pos..].chars().next().unwrap();
                if ch.is_whitespace() {
                    self.consume_char();
                    advanced = true;
                } else {
                    break;
                }
            }

            if self.input[self.pos..].starts_with("//") {
                // Line comment: skip through the newline (or EOF).
                while let Some(ch) = self.consume_char() {
                    if ch == '\n' {
                        break;
                    }
                }
                advanced = true;
            } else if self.input[self.pos..].starts_with("/*") {
                // Block comment: skip through the closing `*/` (or EOF
                // if unterminated — tolerated rather than erroring, since
                // the surrounding parse will fail on the truncated input
                // anyway with a clearer "unexpected end of input").
                self.consume_char(); // '/'
                self.consume_char(); // '*'
                while self.pos < self.input.len() && !self.input[self.pos..].starts_with("*/") {
                    self.consume_char();
                }
                if self.input[self.pos..].starts_with("*/") {
                    self.consume_char(); // '*'
                    self.consume_char(); // '/'
                }
                advanced = true;
            }

            if !advanced {
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
