//! Cypher Parser - AST definitions and parsing logic
//!
//! This module provides the Abstract Syntax Tree (AST) structures and parsing
//! logic for Cypher queries. It supports the MVP subset of Cypher including
//! MATCH, WHERE, RETURN, ORDER BY, LIMIT, and SKIP clauses.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Abstract Syntax Tree for Cypher queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CypherQuery {
    /// Query clauses in order
    pub clauses: Vec<Clause>,
    /// Query parameters
    pub params: HashMap<String, serde_json::Value>,
}

/// Individual query clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Clause {
    /// MATCH clause for pattern matching
    Match(MatchClause),
    /// WHERE clause for filtering
    Where(WhereClause),
    /// RETURN clause for projection
    Return(ReturnClause),
    /// ORDER BY clause for sorting
    OrderBy(OrderByClause),
    /// LIMIT clause for result limiting
    Limit(LimitClause),
    /// SKIP clause for result offset
    Skip(SkipClause),
}

/// MATCH clause with pattern matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchClause {
    /// Pattern to match
    pub pattern: Pattern,
    /// Optional WHERE condition
    pub where_clause: Option<WhereClause>,
}

/// Pattern matching structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Pattern elements (nodes and relationships)
    pub elements: Vec<PatternElement>,
}

/// Pattern element (node or relationship)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternElement {
    /// Node pattern
    Node(NodePattern),
    /// Relationship pattern
    Relationship(RelationshipPattern),
}

/// Node pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePattern {
    /// Variable name (optional)
    pub variable: Option<String>,
    /// Node labels
    pub labels: Vec<String>,
    /// Property map (optional)
    pub properties: Option<PropertyMap>,
}

/// Relationship pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipPattern {
    /// Variable name (optional)
    pub variable: Option<String>,
    /// Relationship types
    pub types: Vec<String>,
    /// Relationship direction
    pub direction: RelationshipDirection,
    /// Property map (optional)
    pub properties: Option<PropertyMap>,
    /// Quantifier (optional)
    pub quantifier: Option<RelationshipQuantifier>,
}

/// Relationship direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationshipDirection {
    /// Outgoing: (a)-[r]->(b)
    Outgoing,
    /// Incoming: (a)<-[r]-(b)
    Incoming,
    /// Both: (a)-[r]-(b)
    Both,
}

/// Relationship quantifier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelationshipQuantifier {
    /// Zero or more: *
    ZeroOrMore,
    /// One or more: +
    OneOrMore,
    /// Zero or one: ?
    ZeroOrOne,
    /// Exact count: {n}
    Exact(usize),
    /// Range: {n,m}
    Range(usize, usize),
}

/// Property map
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyMap {
    /// Property key-value pairs
    pub properties: HashMap<String, Expression>,
}

/// WHERE clause for filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhereClause {
    /// Boolean expression
    pub expression: Expression,
}

/// RETURN clause for projection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnClause {
    /// Return items
    pub items: Vec<ReturnItem>,
    /// DISTINCT modifier
    pub distinct: bool,
}

/// Return item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnItem {
    /// Expression to return
    pub expression: Expression,
    /// Alias (optional)
    pub alias: Option<String>,
}

/// ORDER BY clause for sorting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderByClause {
    /// Sort items
    pub items: Vec<SortItem>,
}

/// Sort item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortItem {
    /// Expression to sort by
    pub expression: Expression,
    /// Sort direction
    pub direction: SortDirection,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortDirection {
    /// Ascending (default)
    Ascending,
    /// Descending
    Descending,
}

/// LIMIT clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitClause {
    /// Limit count
    pub count: Expression,
}

/// SKIP clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkipClause {
    /// Skip count
    pub count: Expression,
}

/// Expression types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expression {
    /// Literal value
    Literal(Literal),
    /// Variable reference
    Variable(String),
    /// Property access (variable.property)
    PropertyAccess {
        /// Variable name
        variable: String,
        /// Property name
        property: String,
    },
    /// Function call
    FunctionCall {
        /// Function name
        name: String,
        /// Function arguments
        args: Vec<Expression>,
    },
    /// Binary operation
    BinaryOp {
        /// Left operand
        left: Box<Expression>,
        /// Operator
        op: BinaryOperator,
        /// Right operand
        right: Box<Expression>,
    },
    /// Unary operation
    UnaryOp {
        /// Operator
        op: UnaryOperator,
        /// Operand
        operand: Box<Expression>,
    },
    /// Parameter reference ($param)
    Parameter(String),
    /// Case expression
    Case {
        /// Input expression (optional)
        input: Option<Box<Expression>>,
        /// When clauses
        when_clauses: Vec<WhenClause>,
        /// Else clause (optional)
        else_clause: Option<Box<Expression>>,
    },
    /// List expression
    List(Vec<Expression>),
    /// Map expression
    Map(HashMap<String, Expression>),
}

/// When clause for CASE expressions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhenClause {
    /// Condition expression
    pub condition: Expression,
    /// Result expression
    pub result: Expression,
}

/// Literal values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Literal {
    /// String literal
    String(String),
    /// Integer literal
    Integer(i64),
    /// Float literal
    Float(f64),
    /// Boolean literal
    Boolean(bool),
    /// Null literal
    Null,
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOperator {
    /// Addition
    Add,
    /// Subtraction
    Subtract,
    /// Multiplication
    Multiply,
    /// Division
    Divide,
    /// Modulo
    Modulo,
    /// Exponentiation
    Power,
    /// Equality
    Equal,
    /// Inequality
    NotEqual,
    /// Less than
    LessThan,
    /// Less than or equal
    LessThanOrEqual,
    /// Greater than
    GreaterThan,
    /// Greater than or equal
    GreaterThanOrEqual,
    /// Logical AND
    And,
    /// Logical OR
    Or,
    /// String concatenation
    Concat,
    /// IN operator
    In,
    /// STARTS WITH
    StartsWith,
    /// ENDS WITH
    EndsWith,
    /// CONTAINS
    Contains,
    /// Regular expression match
    RegexMatch,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnaryOperator {
    /// Logical NOT
    Not,
    /// Unary minus
    Minus,
    /// Unary plus
    Plus,
}

/// Cypher parser
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

impl CypherParser {
    /// Create a new parser
    pub fn new(input: String) -> Self {
        Self {
            pos: 0,
            input,
            line: 1,
            column: 1,
        }
    }

    /// Parse a Cypher query
    pub fn parse(&mut self) -> Result<CypherQuery> {
        let mut clauses = Vec::new();
        let params = HashMap::new();

        // Skip whitespace
        self.skip_whitespace();

        // Parse clauses
        while self.pos < self.input.len() {
            // Check if we're at a clause boundary
            if self.is_clause_boundary() {
                let clause = self.parse_clause()?;
                clauses.push(clause);

                // Skip whitespace
                self.skip_whitespace();

                // Check for end of input
                if self.pos >= self.input.len() {
                    break;
                }
            } else {
                // No more clauses to parse
                break;
            }
        }

        Ok(CypherQuery { clauses, params })
    }

    /// Parse a single clause
    fn parse_clause(&mut self) -> Result<Clause> {
        let keyword = self.parse_keyword()?;

        match keyword.to_uppercase().as_str() {
            "MATCH" => {
                let match_clause = self.parse_match_clause()?;
                Ok(Clause::Match(match_clause))
            }
            "WHERE" => {
                let where_clause = self.parse_where_clause()?;
                Ok(Clause::Where(where_clause))
            }
            "RETURN" => {
                let return_clause = self.parse_return_clause()?;
                Ok(Clause::Return(return_clause))
            }
            "ORDER" => {
                self.expect_keyword("BY")?;
                let order_by_clause = self.parse_order_by_clause()?;
                Ok(Clause::OrderBy(order_by_clause))
            }
            "LIMIT" => {
                let limit_clause = self.parse_limit_clause()?;
                Ok(Clause::Limit(limit_clause))
            }
            "SKIP" => {
                let skip_clause = self.parse_skip_clause()?;
                Ok(Clause::Skip(skip_clause))
            }
            _ => Err(self.error(&format!("Unexpected keyword: {}", keyword))),
        }
    }

    /// Parse MATCH clause
    fn parse_match_clause(&mut self) -> Result<MatchClause> {
        self.skip_whitespace();
        let pattern = self.parse_pattern()?;

        Ok(MatchClause {
            pattern,
            where_clause: None, // WHERE is now a separate clause
        })
    }

    /// Parse pattern
    fn parse_pattern(&mut self) -> Result<Pattern> {
        let mut elements = Vec::new();

        // Parse first node
        let node = self.parse_node_pattern()?;
        elements.push(PatternElement::Node(node));

        // Parse relationships and nodes
        while self.pos < self.input.len() {
            // Check if there's a relationship pattern by looking ahead
            let saved_pos = self.pos;
            let saved_line = self.line;
            let saved_column = self.column;

            // Skip whitespace
            self.skip_whitespace();

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
                // Restore position if no relationship found
                self.pos = saved_pos;
                self.line = saved_line;
                self.column = saved_column;
                break;
            }
        }

        Ok(Pattern { elements })
    }

    /// Parse node pattern
    fn parse_node_pattern(&mut self) -> Result<NodePattern> {
        self.expect_char('(')?;

        let variable = if self.is_identifier_start() {
            Some(self.parse_identifier()?)
        } else {
            None
        };

        let labels = if self.peek_char() == Some(':') {
            self.parse_labels()?
        } else {
            Vec::new()
        };

        let properties = if self.peek_char() == Some('{') {
            Some(self.parse_property_map()?)
        } else {
            None
        };

        self.expect_char(')')?;

        Ok(NodePattern {
            variable,
            labels,
            properties,
        })
    }

    /// Parse relationship pattern
    fn parse_relationship_pattern(&mut self) -> Result<RelationshipPattern> {
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

        let variable = if self.is_identifier_start() {
            Some(self.parse_identifier()?)
        } else {
            None
        };

        let types = if self.peek_char() == Some(':') {
            self.parse_types()?
        } else {
            Vec::new()
        };

        let properties = if self.peek_char() == Some('{') {
            Some(self.parse_property_map()?)
        } else {
            None
        };

        let quantifier = self.parse_relationship_quantifier()?;

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
    fn parse_relationship_direction(&mut self) -> Result<RelationshipDirection> {
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

    /// Parse labels
    fn parse_labels(&mut self) -> Result<Vec<String>> {
        let mut labels = Vec::new();

        while self.peek_char() == Some(':') {
            self.consume_char(); // consume ':'
            let label = self.parse_identifier()?;
            labels.push(label);
        }

        Ok(labels)
    }

    /// Parse types
    fn parse_types(&mut self) -> Result<Vec<String>> {
        let mut types = Vec::new();

        while self.peek_char() == Some(':') {
            self.consume_char(); // consume ':'
            let r#type = self.parse_identifier()?;
            types.push(r#type);
        }

        Ok(types)
    }

    /// Parse property map
    fn parse_property_map(&mut self) -> Result<PropertyMap> {
        self.expect_char('{')?;

        let mut properties = HashMap::new();

        while self.peek_char() != Some('}') {
            let key = self.parse_identifier()?;
            self.expect_char(':')?;
            let value = self.parse_expression()?;
            properties.insert(key, value);

            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            }
        }

        self.expect_char('}')?;

        Ok(PropertyMap { properties })
    }

    /// Parse relationship quantifier
    fn parse_relationship_quantifier(&mut self) -> Result<Option<RelationshipQuantifier>> {
        match self.peek_char() {
            Some('*') => {
                self.consume_char();
                Ok(Some(RelationshipQuantifier::ZeroOrMore))
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

    /// Parse range quantifier
    fn parse_range_quantifier(&mut self) -> Result<Option<RelationshipQuantifier>> {
        self.expect_char('{')?;

        let start = if self.is_digit() {
            Some(self.parse_number()?)
        } else {
            None
        };

        if self.peek_char() == Some(',') {
            self.consume_char();
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

    /// Parse WHERE clause
    fn parse_where_clause(&mut self) -> Result<WhereClause> {
        self.skip_whitespace();
        let expression = self.parse_expression()?;
        Ok(WhereClause { expression })
    }

    /// Parse RETURN clause
    fn parse_return_clause(&mut self) -> Result<ReturnClause> {
        self.skip_whitespace();

        let distinct = if self.peek_keyword("DISTINCT") {
            self.parse_keyword()?;
            true
        } else {
            false
        };

        self.skip_whitespace();

        let mut items = Vec::new();

        loop {
            let item = self.parse_return_item()?;
            items.push(item);

            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        Ok(ReturnClause { items, distinct })
    }

    /// Parse return item
    fn parse_return_item(&mut self) -> Result<ReturnItem> {
        let expression = self.parse_expression()?;

        let alias = if self.peek_keyword("AS") {
            self.parse_keyword()?;
            Some(self.parse_identifier()?)
        } else {
            None
        };

        Ok(ReturnItem { expression, alias })
    }

    /// Parse ORDER BY clause
    fn parse_order_by_clause(&mut self) -> Result<OrderByClause> {
        self.skip_whitespace();

        let mut items = Vec::new();

        loop {
            let expression = self.parse_expression()?;

            let direction = if self.peek_keyword("ASC") {
                self.parse_keyword()?;
                SortDirection::Ascending
            } else if self.peek_keyword("DESC") {
                self.parse_keyword()?;
                SortDirection::Descending
            } else {
                SortDirection::Ascending
            };

            items.push(SortItem {
                expression,
                direction,
            });

            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        Ok(OrderByClause { items })
    }

    /// Parse LIMIT clause
    fn parse_limit_clause(&mut self) -> Result<LimitClause> {
        self.skip_whitespace();
        let count = self.parse_expression()?;
        Ok(LimitClause { count })
    }

    /// Parse SKIP clause
    fn parse_skip_clause(&mut self) -> Result<SkipClause> {
        self.skip_whitespace();
        let count = self.parse_expression()?;
        Ok(SkipClause { count })
    }

    /// Parse expression
    /// Parse expression (simplified for MVP)
    pub fn parse_expression(&mut self) -> Result<Expression> {
        self.skip_whitespace();

        // Try to parse a simple expression
        let mut left = self.parse_simple_expression()?;

        // Check for binary operators (including AND, OR)
        while let Some(op) = self.parse_binary_operator() {
            self.skip_whitespace();
            let right = self.parse_simple_expression()?;
            left = Expression::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
            self.skip_whitespace();
        }

        Ok(left)
    }

    /// Parse simple expression (no binary operators)
    fn parse_simple_expression(&mut self) -> Result<Expression> {
        self.skip_whitespace();

        match self.peek_char() {
            Some('(') => self.parse_parenthesized_expression(),
            Some('$') => self.parse_parameter(),
            Some('"') | Some('\'') => self.parse_string_literal(),
            Some(c) if c.is_ascii_digit() => self.parse_numeric_literal(),
            Some(c) if self.is_identifier_start() => {
                // Check if it's a keyword first
                if self.peek_keyword("CASE") {
                    self.parse_case_expression()
                } else {
                    self.parse_identifier_expression()
                }
            }
            Some('[') => self.parse_list_expression(),
            Some('{') => self.parse_map_expression(),
            _ => Err(self.error("Unexpected character in expression")),
        }
    }

    /// Parse binary operator
    fn parse_binary_operator(&mut self) -> Option<BinaryOperator> {
        self.skip_whitespace();

        // Check for keyword operators first (AND, OR)
        if self.peek_keyword("AND") {
            self.parse_keyword().ok()?;
            return Some(BinaryOperator::And);
        } else if self.peek_keyword("OR") {
            self.parse_keyword().ok()?;
            return Some(BinaryOperator::Or);
        }

        // Then check for symbol operators
        match self.peek_char() {
            Some('=') => {
                self.consume_char();
                Some(BinaryOperator::Equal)
            }
            Some('!') if self.peek_char_at(1) == Some('=') => {
                self.consume_char();
                self.consume_char();
                Some(BinaryOperator::NotEqual)
            }
            Some('<') => {
                self.consume_char();
                if self.peek_char() == Some('=') {
                    self.consume_char();
                    Some(BinaryOperator::LessThanOrEqual)
                } else {
                    Some(BinaryOperator::LessThan)
                }
            }
            Some('>') => {
                self.consume_char();
                if self.peek_char() == Some('=') {
                    self.consume_char();
                    Some(BinaryOperator::GreaterThanOrEqual)
                } else {
                    Some(BinaryOperator::GreaterThan)
                }
            }
            Some('+') => {
                self.consume_char();
                Some(BinaryOperator::Add)
            }
            Some('-') => {
                self.consume_char();
                Some(BinaryOperator::Subtract)
            }
            Some('*') => {
                self.consume_char();
                Some(BinaryOperator::Multiply)
            }
            Some('/') => {
                self.consume_char();
                Some(BinaryOperator::Divide)
            }
            _ => None,
        }
    }

    /// Peek character at specific offset
    fn peek_char_at(&self, offset: usize) -> Option<char> {
        self.input.chars().nth(self.pos + offset)
    }

    /// Parse primary expression
    fn parse_primary_expression(&mut self) -> Result<Expression> {
        self.skip_whitespace();

        match self.peek_char() {
            Some('(') => self.parse_parenthesized_expression(),
            Some('$') => self.parse_parameter(),
            Some('"') | Some('\'') => self.parse_string_literal(),
            Some(c) if c.is_ascii_digit() => self.parse_numeric_literal(),
            Some(c) if self.is_identifier_start() => {
                // Check if it's a keyword first
                if self.peek_keyword("CASE") {
                    self.parse_case_expression()
                } else {
                    self.parse_identifier_expression()
                }
            }
            Some('[') => self.parse_list_expression(),
            Some('{') => self.parse_map_expression(),
            _ => Err(self.error("Unexpected character in expression")),
        }
    }

    /// Parse parenthesized expression
    fn parse_parenthesized_expression(&mut self) -> Result<Expression> {
        self.expect_char('(')?;
        let expr = self.parse_expression()?;
        self.expect_char(')')?;
        Ok(expr)
    }

    /// Parse parameter
    fn parse_parameter(&mut self) -> Result<Expression> {
        self.expect_char('$')?;
        let name = self.parse_identifier()?;
        Ok(Expression::Parameter(name))
    }

    /// Parse string literal
    fn parse_string_literal(&mut self) -> Result<Expression> {
        let quote = self.consume_char().unwrap();
        let mut value = String::new();

        while self.pos < self.input.len() {
            let ch = self.consume_char().unwrap();
            if ch == quote {
                break;
            } else if ch == '\\' && self.pos < self.input.len() {
                let next = self.consume_char().unwrap();
                match next {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    '\\' => value.push('\\'),
                    _ => value.push(next),
                }
            } else {
                value.push(ch);
            }
        }

        Ok(Expression::Literal(Literal::String(value)))
    }

    /// Parse numeric literal
    fn parse_numeric_literal(&mut self) -> Result<Expression> {
        let start = self.pos;

        // Parse integer part
        while self.pos < self.input.len() && self.is_digit() {
            self.consume_char();
        }

        // Check for decimal point
        if self.peek_char() == Some('.') {
            self.consume_char();
            while self.pos < self.input.len() && self.is_digit() {
                self.consume_char();
            }

            // Parse as float
            let value = self.input[start..self.pos]
                .parse::<f64>()
                .map_err(|_| self.error("Invalid float literal"))?;
            Ok(Expression::Literal(Literal::Float(value)))
        } else {
            // Parse as integer
            let value = self.input[start..self.pos]
                .parse::<i64>()
                .map_err(|_| self.error("Invalid integer literal"))?;
            Ok(Expression::Literal(Literal::Integer(value)))
        }
    }

    /// Parse identifier expression
    fn parse_identifier_expression(&mut self) -> Result<Expression> {
        let identifier = self.parse_identifier()?;

        // Check for property access
        if self.peek_char() == Some('.') {
            self.consume_char();
            let property = self.parse_identifier()?;
            Ok(Expression::PropertyAccess {
                variable: identifier,
                property,
            })
        } else {
            Ok(Expression::Variable(identifier))
        }
    }

    /// Parse list expression
    fn parse_list_expression(&mut self) -> Result<Expression> {
        self.expect_char('[')?;

        let mut elements = Vec::new();

        while self.peek_char() != Some(']') {
            let expr = self.parse_expression()?;
            elements.push(expr);

            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            }
        }

        self.expect_char(']')?;
        Ok(Expression::List(elements))
    }

    /// Parse map expression
    fn parse_map_expression(&mut self) -> Result<Expression> {
        let property_map = self.parse_property_map()?;
        Ok(Expression::Map(property_map.properties))
    }

    /// Parse case expression
    fn parse_case_expression(&mut self) -> Result<Expression> {
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

    /// Parse comparison operator
    fn parse_comparison_operator(&mut self) -> Option<BinaryOperator> {
        match self.peek_char() {
            Some('=') => {
                self.consume_char();
                Some(BinaryOperator::Equal)
            }
            Some('!') if self.peek_char_at(1) == Some('=') => {
                self.consume_char();
                self.consume_char();
                Some(BinaryOperator::NotEqual)
            }
            Some('<') => {
                self.consume_char();
                if self.peek_char() == Some('=') {
                    self.consume_char();
                    Some(BinaryOperator::LessThanOrEqual)
                } else {
                    Some(BinaryOperator::LessThan)
                }
            }
            Some('>') => {
                self.consume_char();
                if self.peek_char() == Some('=') {
                    self.consume_char();
                    Some(BinaryOperator::GreaterThanOrEqual)
                } else {
                    Some(BinaryOperator::GreaterThan)
                }
            }
            _ => None,
        }
    }

    /// Parse additive operator
    fn parse_additive_operator(&mut self) -> Option<BinaryOperator> {
        match self.peek_char() {
            Some('+') => {
                self.consume_char();
                Some(BinaryOperator::Add)
            }
            Some('-') => {
                self.consume_char();
                Some(BinaryOperator::Subtract)
            }
            _ => None,
        }
    }

    /// Parse multiplicative operator
    fn parse_multiplicative_operator(&mut self) -> Option<BinaryOperator> {
        match self.peek_char() {
            Some('*') => {
                self.consume_char();
                Some(BinaryOperator::Multiply)
            }
            Some('/') => {
                self.consume_char();
                Some(BinaryOperator::Divide)
            }
            Some('%') => {
                self.consume_char();
                Some(BinaryOperator::Modulo)
            }
            Some('^') => {
                self.consume_char();
                Some(BinaryOperator::Power)
            }
            _ => None,
        }
    }

    /// Parse unary operator
    fn parse_unary_operator(&mut self) -> Option<UnaryOperator> {
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
    fn parse_keyword(&mut self) -> Result<String> {
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
    fn parse_identifier(&mut self) -> Result<String> {
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
    fn parse_number(&mut self) -> Result<i64> {
        let start = self.pos;

        while self.pos < self.input.len() && self.is_digit() {
            self.consume_char();
        }

        self.input[start..self.pos]
            .parse::<i64>()
            .map_err(|_| self.error("Invalid number"))
    }

    /// Check if character is keyword character
    fn is_keyword_char(&self) -> bool {
        self.peek_char()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
    }

    /// Check if we're at a clause boundary
    fn is_clause_boundary(&self) -> bool {
        // Check if we're at the start of a valid clause keyword
        self.peek_keyword("MATCH")
            || self.peek_keyword("WHERE")
            || self.peek_keyword("RETURN")
            || self.peek_keyword("ORDER")
            || self.peek_keyword("LIMIT")
            || self.peek_keyword("SKIP")
    }

    /// Check if character is identifier start
    fn is_identifier_start(&self) -> bool {
        self.peek_char()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
    }

    /// Check if character is identifier character
    fn is_identifier_char(&self) -> bool {
        self.peek_char()
            .map(|c| c.is_ascii_alphanumeric() || c == '_')
            .unwrap_or(false)
    }

    /// Check if character is digit
    fn is_digit(&self) -> bool {
        self.peek_char()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
    }

    /// Peek at current character
    fn peek_char(&self) -> Option<char> {
        self.input.chars().nth(self.pos)
    }

    /// Consume current character
    fn consume_char(&mut self) -> Option<char> {
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
    fn expect_char(&mut self, expected: char) -> Result<()> {
        if self.consume_char() == Some(expected) {
            Ok(())
        } else {
            Err(self.error(&format!("Expected '{}'", expected)))
        }
    }

    /// Expect specific keyword
    fn expect_keyword(&mut self, expected: &str) -> Result<()> {
        let keyword = self.parse_keyword()?;
        if keyword.to_uppercase() == expected.to_uppercase() {
            Ok(())
        } else {
            Err(self.error(&format!("Expected keyword '{}'", expected)))
        }
    }

    /// Check if next token is keyword
    fn peek_keyword(&self, keyword: &str) -> bool {
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
    fn skip_whitespace(&mut self) {
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
    fn error(&self, message: &str) -> Error {
        Error::CypherSyntax(format!(
            "{} at line {}, column {}",
            message, self.line, self.column
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_match() {
        let mut parser = CypherParser::new("MATCH (n:Person) RETURN n".to_string());
        let query = parser.parse().unwrap();

        assert_eq!(query.clauses.len(), 2);

        match &query.clauses[0] {
            Clause::Match(match_clause) => {
                assert_eq!(match_clause.pattern.elements.len(), 1);
                match &match_clause.pattern.elements[0] {
                    PatternElement::Node(node) => {
                        assert_eq!(node.variable, Some("n".to_string()));
                        assert_eq!(node.labels, vec!["Person"]);
                    }
                    _ => panic!("Expected node pattern"),
                }
            }
            _ => panic!("Expected match clause"),
        }

        match &query.clauses[1] {
            Clause::Return(return_clause) => {
                assert_eq!(return_clause.items.len(), 1);
                assert_eq!(return_clause.distinct, false);
            }
            _ => panic!("Expected return clause"),
        }
    }

    #[test]
    fn test_parse_match_with_where() {
        let mut parser =
            CypherParser::new("MATCH (n:Person) WHERE n.age > 18 RETURN n".to_string());
        let query = parser.parse().unwrap();

        assert_eq!(query.clauses.len(), 3);

        match &query.clauses[0] {
            Clause::Match(match_clause) => {
                assert!(match_clause.where_clause.is_none());
            }
            _ => panic!("Expected match clause"),
        }

        match &query.clauses[1] {
            Clause::Where(where_clause) => {
                // Check that it's a binary operation
                match &where_clause.expression {
                    Expression::BinaryOp { op, .. } => {
                        assert_eq!(*op, BinaryOperator::GreaterThan);
                    }
                    _ => panic!("Expected binary operation"),
                }
            }
            _ => panic!("Expected where clause"),
        }
    }

    #[test]
    fn test_parse_relationship_pattern() {
        let mut parser =
            CypherParser::new("MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a, b".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[0] {
            Clause::Match(match_clause) => {
                assert_eq!(match_clause.pattern.elements.len(), 3); // node, rel, node

                match &match_clause.pattern.elements[1] {
                    PatternElement::Relationship(rel) => {
                        assert_eq!(rel.variable, Some("r".to_string()));
                        assert_eq!(rel.types, vec!["KNOWS"]);
                        assert_eq!(rel.direction, RelationshipDirection::Outgoing);
                    }
                    _ => panic!("Expected relationship pattern"),
                }
            }
            _ => panic!("Expected match clause"),
        }
    }

    #[test]
    fn test_parse_return_with_alias() {
        let mut parser =
            CypherParser::new("MATCH (n:Person) RETURN n.name AS person_name".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[1] {
            Clause::Return(return_clause) => {
                assert_eq!(return_clause.items.len(), 1);

                match &return_clause.items[0] {
                    ReturnItem { expression, alias } => {
                        assert_eq!(alias, &Some("person_name".to_string()));

                        match expression {
                            Expression::PropertyAccess { variable, property } => {
                                assert_eq!(variable, "n");
                                assert_eq!(property, "name");
                            }
                            _ => panic!("Expected property access"),
                        }
                    }
                }
            }
            _ => panic!("Expected return clause"),
        }
    }

    #[test]
    fn test_parse_order_by() {
        let mut parser =
            CypherParser::new("MATCH (n:Person) RETURN n ORDER BY n.age DESC".to_string());
        let query = parser.parse().unwrap();

        assert_eq!(query.clauses.len(), 3);

        match &query.clauses[2] {
            Clause::OrderBy(order_clause) => {
                assert_eq!(order_clause.items.len(), 1);
                assert_eq!(order_clause.items[0].direction, SortDirection::Descending);
            }
            _ => panic!("Expected order by clause"),
        }
    }

    #[test]
    fn test_parse_limit_skip() {
        let mut parser = CypherParser::new("MATCH (n:Person) RETURN n SKIP 10 LIMIT 5".to_string());
        let query = parser.parse().unwrap();

        assert_eq!(query.clauses.len(), 4);

        match &query.clauses[2] {
            Clause::Skip(skip_clause) => match &skip_clause.count {
                Expression::Literal(Literal::Integer(10)) => {}
                _ => panic!("Expected integer literal"),
            },
            _ => panic!("Expected skip clause"),
        }

        match &query.clauses[3] {
            Clause::Limit(limit_clause) => match &limit_clause.count {
                Expression::Literal(Literal::Integer(5)) => {}
                _ => panic!("Expected integer literal"),
            },
            _ => panic!("Expected limit clause"),
        }
    }

    #[test]
    fn test_parse_parameter() {
        let mut parser =
            CypherParser::new("MATCH (n:Person) WHERE n.name = $name RETURN n".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[1] {
            Clause::Where(where_clause) => match &where_clause.expression {
                Expression::BinaryOp { right, .. } => match right.as_ref() {
                    Expression::Parameter(name) => {
                        assert_eq!(name, "name");
                    }
                    _ => panic!("Expected parameter"),
                },
                _ => panic!("Expected binary operation"),
            },
            _ => panic!("Expected where clause"),
        }
    }

    #[test]
    fn test_debug_binary_expression() {
        let mut parser = CypherParser::new("n.age < 18".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::BinaryOp { left, op, right } => {
                assert_eq!(op, BinaryOperator::LessThan);
                match *left {
                    Expression::PropertyAccess { variable, property } => {
                        assert_eq!(variable, "n");
                        assert_eq!(property, "age");
                    }
                    _ => panic!("Expected property access"),
                }
                match *right {
                    Expression::Literal(Literal::Integer(value)) => {
                        assert_eq!(value, 18);
                    }
                    _ => panic!("Expected integer literal"),
                }
            }
            _ => panic!("Expected binary operation"),
        }
    }

    #[test]
    fn test_debug_case_expression() {
        let mut parser =
            CypherParser::new("CASE WHEN n.age < 18 THEN 'minor' ELSE 'adult' END".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::Case {
                when_clauses,
                else_clause,
                ..
            } => {
                assert_eq!(when_clauses.len(), 1);
                assert!(else_clause.is_some());
            }
            _ => panic!("Expected case expression"),
        }
    }

    #[test]
    fn test_debug_when_keyword() {
        let mut parser =
            CypherParser::new("WHEN n.age < 18 THEN 'minor' ELSE 'adult' END".to_string());

        // Test parsing WHEN keyword
        assert!(parser.peek_keyword("WHEN"));
        parser.expect_keyword("WHEN").unwrap();

        // Debug: print remaining input after WHEN
        let remaining = &parser.input[parser.pos..];
        println!("Remaining after WHEN: '{}'", remaining);

        // Test parsing the condition
        let condition = parser.parse_expression().unwrap();
        match condition {
            Expression::BinaryOp {
                left: _,
                op,
                right: _,
            } => {
                assert_eq!(op, BinaryOperator::LessThan);
            }
            _ => panic!("Expected binary operation"),
        }

        // Debug: print remaining input after condition
        let remaining = &parser.input[parser.pos..];
        println!("Remaining after condition: '{}'", remaining);

        // Debug: test peek_keyword for THEN
        println!("peek_keyword('THEN'): {}", parser.peek_keyword("THEN"));

        // Test parsing THEN keyword
        assert!(parser.peek_keyword("THEN"));
        parser.expect_keyword("THEN").unwrap();
    }

    #[test]
    fn test_parse_case_expression() {
        // Test simple binary expression first
        let mut parser = CypherParser::new("n.age < 18".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::BinaryOp { left, op, right } => {
                assert_eq!(op, BinaryOperator::LessThan);
                match *left {
                    Expression::PropertyAccess { variable, property } => {
                        assert_eq!(variable, "n");
                        assert_eq!(property, "age");
                    }
                    _ => panic!("Expected property access"),
                }
                match *right {
                    Expression::Literal(Literal::Integer(value)) => {
                        assert_eq!(value, 18);
                    }
                    _ => panic!("Expected integer literal"),
                }
            }
            _ => panic!("Expected binary operation"),
        }

        // Test simple CASE expression
        let mut parser =
            CypherParser::new("CASE WHEN n.age < 18 THEN 'minor' ELSE 'adult' END".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::Case {
                when_clauses,
                else_clause,
                ..
            } => {
                assert_eq!(when_clauses.len(), 1);
                assert!(else_clause.is_some());
            }
            _ => panic!("Expected case expression"),
        }

        // Now test full query
        let mut parser = CypherParser::new("MATCH (n:Person) RETURN CASE WHEN n.age < 18 THEN 'minor' ELSE 'adult' END AS category".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[1] {
            Clause::Return(return_clause) => match &return_clause.items[0].expression {
                Expression::Case {
                    when_clauses,
                    else_clause,
                    ..
                } => {
                    assert_eq!(when_clauses.len(), 1);
                    assert!(else_clause.is_some());
                }
                _ => panic!("Expected case expression"),
            },
            _ => panic!("Expected return clause"),
        }
    }

    #[test]
    fn test_parse_error_reporting() {
        let mut parser = CypherParser::new("MATCH (n:Person RETURN n".to_string());
        let result = parser.parse();

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.to_string().contains("line"));
        assert!(error.to_string().contains("column"));
    }

    #[test]
    fn test_parse_complex_query() {
        let query_str = r#"
            MATCH (p:Person)-[r:KNOWS]->(f:Person)
            WHERE p.age > $min_age AND f.city = $city
            RETURN p.name AS person_name, f.name AS friend_name, r.since AS friendship_since
            ORDER BY friendship_since DESC
            LIMIT 10
        "#;

        let mut parser = CypherParser::new(query_str.to_string());
        let query = parser.parse().unwrap();

        assert_eq!(query.clauses.len(), 5); // MATCH, WHERE, RETURN, ORDER BY, LIMIT

        // Verify all clause types are present
        let clause_types: Vec<&str> = query
            .clauses
            .iter()
            .map(|c| match c {
                Clause::Match(_) => "MATCH",
                Clause::Where(_) => "WHERE",
                Clause::Return(_) => "RETURN",
                Clause::OrderBy(_) => "ORDER BY",
                Clause::Limit(_) => "LIMIT",
                _ => "OTHER",
            })
            .collect();

        assert_eq!(
            clause_types,
            vec!["MATCH", "WHERE", "RETURN", "ORDER BY", "LIMIT"]
        );
    }
}
