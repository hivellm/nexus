//! Cypher Parser - AST definitions and parsing logic
//!
//! This module provides the Abstract Syntax Tree (AST) structures and parsing
//! logic for Cypher queries. It supports the MVP subset of Cypher including
//! MATCH, CREATE, WHERE, RETURN, ORDER BY, LIMIT, and SKIP clauses.

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
    /// CREATE clause for creating nodes and relationships
    Create(CreateClause),
    /// MERGE clause for match-or-create
    Merge(MergeClause),
    /// SET clause for updating properties and labels
    Set(SetClause),
    /// DELETE clause for deleting nodes and relationships
    Delete(DeleteClause),
    /// REMOVE clause for removing properties and labels
    Remove(RemoveClause),
    /// WITH clause for query composition and projection
    With(WithClause),
    /// UNWIND clause for list expansion
    Unwind(UnwindClause),
    /// UNION clause for combining multiple query results
    Union(UnionClause),
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
    /// FOREACH clause for iterating over lists
    Foreach(ForeachClause),
    /// CREATE DATABASE command
    CreateDatabase(CreateDatabaseClause),
    /// DROP DATABASE command
    DropDatabase(DropDatabaseClause),
    /// SHOW DATABASES command
    ShowDatabases,
    /// USE DATABASE command
    UseDatabase(UseDatabaseClause),
    /// BEGIN TRANSACTION command
    BeginTransaction,
    /// COMMIT TRANSACTION command
    CommitTransaction,
    /// ROLLBACK TRANSACTION command
    RollbackTransaction,
    /// CREATE INDEX command
    CreateIndex(CreateIndexClause),
    /// DROP INDEX command
    DropIndex(DropIndexClause),
    /// CREATE CONSTRAINT command
    CreateConstraint(CreateConstraintClause),
    /// DROP CONSTRAINT command
    DropConstraint(DropConstraintClause),
    /// SHOW USERS command
    ShowUsers,
    /// SHOW USER command (singular)
    ShowUser(ShowUserClause),
    /// CREATE USER command
    CreateUser(CreateUserClause),
    /// DROP USER command
    DropUser(DropUserClause),
    /// GRANT command
    Grant(GrantClause),
    /// REVOKE command
    Revoke(RevokeClause),
    /// CREATE API KEY command
    CreateApiKey(CreateApiKeyClause),
    /// SHOW API KEYS command
    ShowApiKeys(ShowApiKeysClause),
    /// REVOKE API KEY command
    RevokeApiKey(RevokeApiKeyClause),
    /// DELETE API KEY command
    DeleteApiKey(DeleteApiKeyClause),
    /// EXPLAIN command for query plan analysis
    Explain(ExplainClause),
    /// PROFILE command for query execution profiling
    Profile(ProfileClause),
    /// CALL subquery clause
    CallSubquery(CallSubqueryClause),
}

/// MATCH clause with pattern matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchClause {
    /// Pattern to match
    pub pattern: Pattern,
    /// Optional WHERE condition
    pub where_clause: Option<WhereClause>,
    /// Whether this is an OPTIONAL MATCH
    pub optional: bool,
    /// Query hints (USING INDEX, USING SCAN, USING JOIN)
    pub hints: Vec<QueryHint>,
}

/// Query hint for planner optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryHint {
    /// USING INDEX hint: MATCH (n:Label) USING INDEX n:Label(property)
    UsingIndex {
        /// Variable name
        variable: String,
        /// Label name
        label: String,
        /// Property name
        property: String,
    },
    /// USING SCAN hint: MATCH (n:Label) USING SCAN n:Label
    UsingScan {
        /// Variable name
        variable: String,
        /// Label name
        label: String,
    },
    /// USING JOIN hint: MATCH (a)-[r]->(b) USING JOIN ON r
    UsingJoin {
        /// Variable name (relationship or node)
        variable: String,
    },
}

/// CREATE clause for creating nodes and relationships
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateClause {
    /// Pattern to create
    pub pattern: Pattern,
}

/// MERGE clause for match-or-create operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeClause {
    /// Pattern to match or create
    pub pattern: Pattern,
    /// SET operations to execute when creating a new node
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_create: Option<SetClause>,
    /// SET operations to execute when matching an existing node
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_match: Option<SetClause>,
}

/// SET clause for updating properties and adding labels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetClause {
    /// Items to set (property assignments and label additions)
    pub items: Vec<SetItem>,
}

/// SET item (property assignment or label addition)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SetItem {
    /// Set a property value
    Property {
        /// Target (variable)
        target: String,
        /// Property key
        property: String,
        /// Value to set
        value: Expression,
    },
    /// Add a label
    Label {
        /// Target variable
        target: String,
        /// Label to add
        label: String,
    },
}

/// DELETE clause for deleting nodes and relationships
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteClause {
    /// Items to delete (variables)
    pub items: Vec<String>,
    /// Whether to use DETACH DELETE (remove relationships before deleting)
    pub detach: bool,
}

/// REMOVE clause for removing properties and labels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveClause {
    /// Items to remove (property names or labels)
    pub items: Vec<RemoveItem>,
}

/// REMOVE item (property or label removal)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RemoveItem {
    /// Remove a property
    Property {
        /// Target variable
        target: String,
        /// Property key to remove
        property: String,
    },
    /// Remove a label
    Label {
        /// Target variable
        target: String,
        /// Label to remove
        label: String,
    },
}

/// Pattern matching structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Pattern elements (nodes and relationships)
    pub elements: Vec<PatternElement>,
    /// Optional path variable assignment (e.g., p = (a)-[*]-(b))
    pub path_variable: Option<String>,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// WITH clause for query composition and projection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithClause {
    /// With items (expressions to project)
    pub items: Vec<ReturnItem>,
    /// DISTINCT modifier
    pub distinct: bool,
    /// Optional WHERE clause for filtering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub where_clause: Option<WhereClause>,
}

/// UNWIND clause for list expansion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnwindClause {
    /// Expression to unwind (list)
    pub expression: Expression,
    /// Variable to bind each item to
    pub variable: String,
}

/// UNION clause for combining query results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnionClause {
    /// Union type (distinct or all)
    pub union_type: UnionType,
}

/// Union type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnionType {
    /// UNION - distinct results only
    Distinct,
    /// UNION ALL - keep duplicates
    All,
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

/// FOREACH clause for iterating over lists
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeachClause {
    /// Variable to bind each list item to
    pub variable: String,
    /// Expression that evaluates to a list
    pub list_expression: Expression,
    /// Update clauses to execute for each item (SET or DELETE)
    pub update_clauses: Vec<ForeachUpdateClause>,
}

/// Update clause types allowed in FOREACH
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ForeachUpdateClause {
    /// SET clause
    Set(SetClause),
    /// DELETE clause
    Delete(DeleteClause),
}

/// CREATE DATABASE clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDatabaseClause {
    /// Database name
    pub name: String,
    /// Optional IF NOT EXISTS flag
    pub if_not_exists: bool,
}

/// DROP DATABASE clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropDatabaseClause {
    /// Database name
    pub name: String,
    /// Optional IF EXISTS flag
    pub if_exists: bool,
}

/// USE DATABASE clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UseDatabaseClause {
    /// Database name
    pub name: String,
}

/// CREATE INDEX clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateIndexClause {
    /// Label name
    pub label: String,
    /// Property name
    pub property: String,
    /// Optional IF NOT EXISTS flag
    pub if_not_exists: bool,
    /// Optional OR REPLACE flag
    pub or_replace: bool,
}

/// DROP INDEX clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropIndexClause {
    /// Label name
    pub label: String,
    /// Property name
    pub property: String,
    /// Optional IF EXISTS flag
    pub if_exists: bool,
}

/// CREATE CONSTRAINT clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConstraintClause {
    /// Constraint type
    pub constraint_type: ConstraintType,
    /// Label name
    pub label: String,
    /// Property name
    pub property: String,
    /// Optional IF NOT EXISTS flag
    pub if_not_exists: bool,
}

/// Constraint type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintType {
    /// UNIQUE constraint
    Unique,
    /// EXISTS constraint (property must exist)
    Exists,
}

/// DROP CONSTRAINT clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropConstraintClause {
    /// Constraint type
    pub constraint_type: ConstraintType,
    /// Label name
    pub label: String,
    /// Property name
    pub property: String,
    /// Optional IF EXISTS flag
    pub if_exists: bool,
}

/// CREATE USER clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserClause {
    /// Username
    pub username: String,
    /// Password (optional, can be set later)
    pub password: Option<String>,
    /// Optional IF NOT EXISTS flag
    pub if_not_exists: bool,
}

/// DROP USER clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropUserClause {
    /// Username
    pub username: String,
    /// Optional IF EXISTS flag
    pub if_exists: bool,
}

/// SHOW USER clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowUserClause {
    /// Username
    pub username: String,
}

/// GRANT clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantClause {
    /// Permission(s) to grant
    pub permissions: Vec<String>,
    /// Role or user to grant to
    pub target: String,
}

/// REVOKE clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeClause {
    /// Permission(s) to revoke
    pub permissions: Vec<String>,
    /// Role or user to revoke from
    pub target: String,
}

/// CREATE API KEY clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyClause {
    /// Key name
    pub name: String,
    /// User ID (optional, if FOR username is specified)
    pub user_id: Option<String>,
    /// Permissions (optional)
    pub permissions: Vec<String>,
    /// Expiration duration (optional, e.g., "7d", "24h", "30m")
    pub expires_in: Option<String>,
}

/// SHOW API KEYS clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowApiKeysClause {
    /// User ID (optional, if FOR username is specified)
    pub user_id: Option<String>,
}

/// REVOKE API KEY clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeApiKeyClause {
    /// Key ID
    pub key_id: String,
    /// Revocation reason (optional)
    pub reason: Option<String>,
}

/// DELETE API KEY clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteApiKeyClause {
    /// Key ID
    pub key_id: String,
}

/// EXPLAIN clause for query plan analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainClause {
    /// The query to explain
    pub query: CypherQuery,
    /// Original query string (for execution)
    #[serde(skip)]
    pub query_string: Option<String>,
}

/// PROFILE clause for query execution profiling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileClause {
    /// The query to profile
    pub query: CypherQuery,
    /// Original query string (for execution)
    #[serde(skip)]
    pub query_string: Option<String>,
}

/// CALL subquery clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSubqueryClause {
    /// The subquery to execute
    pub query: CypherQuery,
    /// Whether to execute in transactions (CALL ... IN TRANSACTIONS)
    pub in_transactions: bool,
    /// Batch size for IN TRANSACTIONS (optional)
    pub batch_size: Option<usize>,
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
    /// IS NULL / IS NOT NULL check
    IsNull {
        /// Expression to check
        expr: Box<Expression>,
        /// Whether this is IS NOT NULL (true) or IS NULL (false)
        negated: bool,
    },
    /// EXISTS subquery - checks if a pattern exists
    Exists {
        /// Pattern to check for existence
        pattern: Pattern,
        /// Optional WHERE clause for filtering
        where_clause: Option<Box<Expression>>,
    },
    /// Map projection - projects properties from a node/map
    MapProjection {
        /// Source expression (variable or expression)
        source: Box<Expression>,
        /// Projection items (properties to include)
        items: Vec<MapProjectionItem>,
    },
    /// List comprehension - filters and transforms lists
    ListComprehension {
        /// Variable to bind each list item to
        variable: String,
        /// Expression that evaluates to a list
        list_expression: Box<Expression>,
        /// Optional WHERE clause for filtering
        where_clause: Option<Box<Expression>>,
        /// Optional transformation expression (after |)
        transform_expression: Option<Box<Expression>>,
    },
    /// Pattern comprehension - collects patterns and transforms results
    PatternComprehension {
        /// Pattern to match
        pattern: Pattern,
        /// Optional WHERE clause for filtering
        where_clause: Option<Box<Expression>>,
        /// Optional transformation expression (after |)
        transform_expression: Option<Box<Expression>>,
    },
}

/// When clause for CASE expressions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhenClause {
    /// Condition expression
    pub condition: Expression,
    /// Result expression
    pub result: Expression,
}

/// Map projection item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MapProjectionItem {
    /// Property projection: .name or .name AS alias
    Property {
        /// Property name (without the dot)
        property: String,
        /// Optional alias
        alias: Option<String>,
    },
    /// Virtual key: name: expression
    VirtualKey {
        /// Key name
        key: String,
        /// Expression to evaluate
        expression: Expression,
    },
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

        // Allow empty queries (for EXPLAIN/PROFILE nested queries)
        // The planner will validate if needed
        Ok(CypherQuery { clauses, params })
    }

    /// Parse a single clause
    fn parse_clause(&mut self) -> Result<Clause> {
        // Check for EXPLAIN or PROFILE first (must be at the beginning)
        if self.peek_keyword("EXPLAIN") {
            return self.parse_explain_clause();
        }
        if self.peek_keyword("PROFILE") {
            return self.parse_profile_clause();
        }

        // Check for OPTIONAL MATCH first
        if self.peek_keyword("OPTIONAL") {
            self.parse_keyword()?; // consume "OPTIONAL"
            self.expect_keyword("MATCH")?;
            let mut match_clause = self.parse_match_clause()?;
            match_clause.optional = true;
            return Ok(Clause::Match(match_clause));
        }

        let keyword = self.parse_keyword()?;

        // Check for DETACH DELETE after reading keyword
        if keyword.to_uppercase() == "DETACH" {
            self.expect_keyword("DELETE")?;
            self.skip_whitespace();

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

            return Ok(Clause::Delete(DeleteClause {
                items,
                detach: true,
            }));
        }

        match keyword.to_uppercase().as_str() {
            "MATCH" => {
                let match_clause = self.parse_match_clause()?;
                Ok(Clause::Match(match_clause))
            }
            "CREATE" => {
                self.skip_whitespace();
                // Check for CREATE DATABASE/INDEX/CONSTRAINT/USER/API KEY before regular CREATE
                if self.peek_keyword("DATABASE") {
                    let create_db_clause = self.parse_create_database_clause()?;
                    Ok(Clause::CreateDatabase(create_db_clause))
                } else if self.peek_keyword("INDEX") {
                    let create_index_clause = self.parse_create_index_clause()?;
                    Ok(Clause::CreateIndex(create_index_clause))
                } else if self.peek_keyword("CONSTRAINT") {
                    let create_constraint_clause = self.parse_create_constraint_clause()?;
                    Ok(Clause::CreateConstraint(create_constraint_clause))
                } else if self.peek_keyword("USER") {
                    let create_user_clause = self.parse_create_user_clause()?;
                    Ok(Clause::CreateUser(create_user_clause))
                } else if self.peek_keyword("API") {
                    self.parse_keyword()?; // consume "API"
                    self.expect_keyword("KEY")?;
                    let create_api_key_clause = self.parse_create_api_key_clause()?;
                    Ok(Clause::CreateApiKey(create_api_key_clause))
                } else {
                    // Regular CREATE clause
                    let create_clause = self.parse_create_clause()?;
                    Ok(Clause::Create(create_clause))
                }
            }
            "MERGE" => {
                let merge_clause = self.parse_merge_clause()?;
                Ok(Clause::Merge(merge_clause))
            }
            "SET" => {
                let set_clause = self.parse_set_clause()?;
                Ok(Clause::Set(set_clause))
            }
            "DELETE" => {
                self.skip_whitespace();
                // Check if this is DELETE API KEY or regular DELETE
                if self.peek_keyword("API") {
                    self.parse_keyword()?; // consume "API"
                    self.expect_keyword("KEY")?;
                    let delete_api_key_clause = self.parse_delete_api_key_clause()?;
                    Ok(Clause::DeleteApiKey(delete_api_key_clause))
                } else {
                    let delete_clause = self.parse_delete_clause()?;
                    Ok(Clause::Delete(delete_clause))
                }
            }
            "REMOVE" => {
                let remove_clause = self.parse_remove_clause()?;
                Ok(Clause::Remove(remove_clause))
            }
            "WITH" => {
                let with_clause = self.parse_with_clause()?;
                Ok(Clause::With(with_clause))
            }
            "UNWIND" => {
                let unwind_clause = self.parse_unwind_clause()?;
                Ok(Clause::Unwind(unwind_clause))
            }
            "UNION" => {
                let union_clause = self.parse_union_clause()?;
                Ok(Clause::Union(union_clause))
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
            "FOREACH" => {
                let foreach_clause = self.parse_foreach_clause()?;
                Ok(Clause::Foreach(foreach_clause))
            }
            "SHOW" => {
                self.skip_whitespace();
                if self.peek_keyword("DATABASES") {
                    self.parse_keyword()?; // consume "DATABASES"
                    Ok(Clause::ShowDatabases)
                } else if self.peek_keyword("USERS") {
                    self.parse_keyword()?; // consume "USERS"
                    Ok(Clause::ShowUsers)
                } else if self.peek_keyword("USER") {
                    let show_user_clause = self.parse_show_user_clause()?;
                    Ok(Clause::ShowUser(show_user_clause))
                } else if self.peek_keyword("API") {
                    self.parse_keyword()?; // consume "API"
                    self.expect_keyword("KEYS")?;
                    let show_api_keys_clause = self.parse_show_api_keys_clause()?;
                    Ok(Clause::ShowApiKeys(show_api_keys_clause))
                } else {
                    Err(self.error("SHOW must be followed by DATABASES, USERS, USER, or API KEYS"))
                }
            }
            "USE" => {
                self.skip_whitespace();
                if self.peek_keyword("DATABASE") {
                    let use_db_clause = self.parse_use_database_clause()?;
                    Ok(Clause::UseDatabase(use_db_clause))
                } else {
                    Err(self.error("USE must be followed by DATABASE"))
                }
            }
            "CALL" => {
                self.skip_whitespace();
                // Check if this is CALL { subquery } or CALL procedure()
                if self.peek_char() == Some('{') {
                    let call_subquery_clause = self.parse_call_subquery_clause()?;
                    Ok(Clause::CallSubquery(call_subquery_clause))
                } else {
                    // This is a procedure call, not a subquery
                    // For now, we'll return an error as procedure calls are not fully implemented
                    Err(self.error("CALL procedures are not yet supported. Use CALL { subquery } for subqueries."))
                }
            }
            "BEGIN" => {
                self.skip_whitespace();
                if self.peek_keyword("TRANSACTION") {
                    self.parse_keyword()?; // consume "TRANSACTION"
                }
                Ok(Clause::BeginTransaction)
            }
            "COMMIT" => {
                self.skip_whitespace();
                if self.peek_keyword("TRANSACTION") {
                    self.parse_keyword()?; // consume "TRANSACTION"
                }
                Ok(Clause::CommitTransaction)
            }
            "ROLLBACK" => {
                self.skip_whitespace();
                if self.peek_keyword("TRANSACTION") {
                    self.parse_keyword()?; // consume "TRANSACTION"
                }
                Ok(Clause::RollbackTransaction)
            }
            "DROP" => {
                self.skip_whitespace();
                if self.peek_keyword("DATABASE") {
                    let drop_db_clause = self.parse_drop_database_clause()?;
                    Ok(Clause::DropDatabase(drop_db_clause))
                } else if self.peek_keyword("USER") {
                    let drop_user_clause = self.parse_drop_user_clause()?;
                    Ok(Clause::DropUser(drop_user_clause))
                } else if self.peek_keyword("INDEX") {
                    let drop_index_clause = self.parse_drop_index_clause()?;
                    Ok(Clause::DropIndex(drop_index_clause))
                } else if self.peek_keyword("CONSTRAINT") {
                    let drop_constraint_clause = self.parse_drop_constraint_clause()?;
                    Ok(Clause::DropConstraint(drop_constraint_clause))
                } else {
                    Err(self.error("DROP must be followed by DATABASE, USER, INDEX, or CONSTRAINT"))
                }
            }
            "GRANT" => {
                let grant_clause = self.parse_grant_clause()?;
                Ok(Clause::Grant(grant_clause))
            }
            "REVOKE" => {
                self.skip_whitespace();
                // Check if this is REVOKE API KEY or regular REVOKE
                if self.peek_keyword("API") {
                    self.parse_keyword()?; // consume "API"
                    self.expect_keyword("KEY")?;
                    let revoke_api_key_clause = self.parse_revoke_api_key_clause()?;
                    Ok(Clause::RevokeApiKey(revoke_api_key_clause))
                } else {
                    let revoke_clause = self.parse_revoke_clause()?;
                    Ok(Clause::Revoke(revoke_clause))
                }
            }
            _ => Err(self.error(&format!("Unexpected keyword: {}", keyword))),
        }
    }

    /// Parse MATCH clause
    fn parse_match_clause(&mut self) -> Result<MatchClause> {
        self.skip_whitespace();
        
        // Check for path variable assignment: p = (pattern)
        let path_variable = if self.is_identifier_start() {
            let saved_pos = self.pos;
            let var_name = self.parse_identifier()?;
            self.skip_whitespace();
            
            if self.peek_char() == Some('=') {
                // This is a path variable assignment
                self.consume_char(); // consume '='
                self.skip_whitespace();
                Some(var_name)
            } else {
                // Not a path variable, restore position
                self.pos = saved_pos;
                None
            }
        } else {
            None
        };
        
        let mut pattern = self.parse_pattern()?;
        
        // Set path variable if detected
        if let Some(path_var) = path_variable {
            pattern.path_variable = Some(path_var);
        }

        // Parse hints after pattern: USING INDEX, USING SCAN, USING JOIN
        let mut hints = Vec::new();
        self.skip_whitespace();
        
        while self.peek_keyword("USING") {
            self.parse_keyword()?; // consume "USING"
            self.skip_whitespace();
            
            if self.peek_keyword("INDEX") {
                self.parse_keyword()?; // consume "INDEX"
                self.skip_whitespace();
                
                // Parse: variable:Label(property)
                let variable = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_char(':')?;
                let label = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_char('(')?;
                let property = self.parse_identifier()?;
                self.expect_char(')')?;
                
                hints.push(QueryHint::UsingIndex {
                    variable,
                    label,
                    property,
                });
            } else if self.peek_keyword("SCAN") {
                self.parse_keyword()?; // consume "SCAN"
                self.skip_whitespace();
                
                // Parse: variable:Label
                let variable = self.parse_identifier()?;
                self.skip_whitespace();
                self.expect_char(':')?;
                let label = self.parse_identifier()?;
                
                hints.push(QueryHint::UsingScan {
                    variable,
                    label,
                });
            } else if self.peek_keyword("JOIN") {
                self.parse_keyword()?; // consume "JOIN"
                self.skip_whitespace();
                
                if self.peek_keyword("ON") {
                    self.parse_keyword()?; // consume "ON"
                    self.skip_whitespace();
                }
                
                // Parse: variable
                let variable = self.parse_identifier()?;
                
                hints.push(QueryHint::UsingJoin {
                    variable,
                });
            } else {
                return Err(self.error("USING must be followed by INDEX, SCAN, or JOIN"));
            }
            
            self.skip_whitespace();
        }

        Ok(MatchClause {
            pattern,
            where_clause: None, // WHERE is now a separate clause
            optional: false,    // Set by caller if this is OPTIONAL MATCH
            hints,
        })
    }

    /// Parse CREATE clause
    fn parse_create_clause(&mut self) -> Result<CreateClause> {
        self.skip_whitespace();
        let pattern = self.parse_pattern()?;

        Ok(CreateClause { pattern })
    }

    /// Parse MERGE clause
    fn parse_merge_clause(&mut self) -> Result<MergeClause> {
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

        Ok(MergeClause {
            pattern,
            on_create,
            on_match,
        })
    }

    /// Parse SET clause
    fn parse_set_clause(&mut self) -> Result<SetClause> {
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
                // Label addition (node:Label)
                self.consume_char();
                let label = self.parse_identifier()?;
                items.push(SetItem::Label { target, label });
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
    fn parse_delete_clause(&mut self) -> Result<DeleteClause> {
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
    fn parse_remove_clause(&mut self) -> Result<RemoveClause> {
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
                // Label removal (node:Label)
                self.consume_char();
                let label = self.parse_identifier()?;
                items.push(RemoveItem::Label { target, label });
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

    /// Parse pattern
    fn parse_pattern(&mut self) -> Result<Pattern> {
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

    /// Parse node pattern
    fn parse_node_pattern(&mut self) -> Result<NodePattern> {
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

    /// Parse WITH clause
    fn parse_with_clause(&mut self) -> Result<WithClause> {
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

        // Check for WHERE clause in WITH
        let where_clause = if self.peek_keyword("WHERE") {
            self.parse_keyword()?;
            Some(self.parse_where_clause()?.clone())
        } else {
            None
        };

        Ok(WithClause {
            items,
            distinct,
            where_clause,
        })
    }

    /// Parse UNWIND clause
    fn parse_unwind_clause(&mut self) -> Result<UnwindClause> {
        self.skip_whitespace();

        // Parse the expression (list to unwind)
        let expression = self.parse_expression()?;

        self.skip_whitespace();

        // Expect AS keyword
        self.expect_keyword("AS")?;

        self.skip_whitespace();

        // Parse the variable name
        let variable = self.parse_identifier()?;

        Ok(UnwindClause {
            expression,
            variable,
        })
    }

    /// Parse FOREACH clause
    fn parse_foreach_clause(&mut self) -> Result<ForeachClause> {
        self.skip_whitespace();

        // Expect opening parenthesis
        self.expect_char('(')?;
        self.skip_whitespace();

        // Parse variable name
        let variable = self.parse_identifier()?;
        self.skip_whitespace();

        // Expect IN keyword
        self.expect_keyword("IN")?;
        self.skip_whitespace();

        // Parse list expression
        let list_expression = self.parse_expression()?;
        self.skip_whitespace();

        // Expect pipe separator |
        self.expect_char('|')?;
        self.skip_whitespace();

        // Parse update clauses (SET or DELETE) until closing parenthesis
        let mut update_clauses = Vec::new();
        loop {
            self.skip_whitespace();

            // Check for closing parenthesis
            if self.peek_char() == Some(')') {
                self.consume_char();
                break;
            }

            // Parse SET or DELETE clause
            let keyword = self.parse_keyword()?;
            match keyword.to_uppercase().as_str() {
                "SET" => {
                    let set_clause = self.parse_set_clause()?;
                    update_clauses.push(ForeachUpdateClause::Set(set_clause));
                }
                "DELETE" => {
                    // Check for DETACH DELETE
                    let detach = if self.peek_keyword("DETACH") {
                        self.parse_keyword()?;
                        self.skip_whitespace();
                        true
                    } else {
                        false
                    };

                    self.skip_whitespace();
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

                    let delete_clause = DeleteClause { items, detach };
                    update_clauses.push(ForeachUpdateClause::Delete(delete_clause));
                }
                _ => {
                    return Err(self.error(&format!(
                        "Expected SET or DELETE in FOREACH, found: {}",
                        keyword
                    )));
                }
            }

            self.skip_whitespace();
        }

        Ok(ForeachClause {
            variable,
            list_expression,
            update_clauses,
        })
    }

    /// Parse UNION clause
    fn parse_union_clause(&mut self) -> Result<UnionClause> {
        self.skip_whitespace();

        // Check for ALL keyword
        let union_type = if self.peek_keyword("ALL") {
            self.parse_keyword()?;
            UnionType::All
        } else {
            UnionType::Distinct
        };

        Ok(UnionClause { union_type })
    }

    /// Parse CREATE DATABASE clause
    /// Syntax: CREATE DATABASE name [IF NOT EXISTS]
    fn parse_create_database_clause(&mut self) -> Result<CreateDatabaseClause> {
        self.expect_keyword("DATABASE")?;
        self.skip_whitespace();

        // Check for IF NOT EXISTS
        let if_not_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        let name = self.parse_identifier()?;
        Ok(CreateDatabaseClause {
            name,
            if_not_exists,
        })
    }

    /// Parse DROP DATABASE clause
    /// Syntax: DROP DATABASE name [IF EXISTS]
    fn parse_drop_database_clause(&mut self) -> Result<DropDatabaseClause> {
        self.expect_keyword("DATABASE")?;
        self.skip_whitespace();

        // Check for IF EXISTS
        let if_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        let name = self.parse_identifier()?;
        Ok(DropDatabaseClause { name, if_exists })
    }

    /// Parse USE DATABASE clause
    /// Syntax: USE DATABASE name
    fn parse_use_database_clause(&mut self) -> Result<UseDatabaseClause> {
        self.expect_keyword("DATABASE")?;
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        Ok(UseDatabaseClause { name })
    }

    /// Parse CALL subquery clause
    /// Syntax: CALL { subquery } [IN TRANSACTIONS [OF n ROWS]]
    fn parse_call_subquery_clause(&mut self) -> Result<CallSubqueryClause> {
        // Expect opening brace
        self.expect_char('{')?;
        self.skip_whitespace();

        // Parse the subquery (nested CypherQuery)
        let mut clauses = Vec::new();

        // Parse clauses until we find the closing brace
        while self.pos < self.input.len() {
            self.skip_whitespace();
            
            // Check for closing brace
            if self.peek_char() == Some('}') {
                self.consume_char();
                break;
            }

            // Check if this is a clause boundary
            if self.is_clause_boundary() {
                let clause = self.parse_clause()?;
                clauses.push(clause);
            } else {
                // No more clauses
                break;
            }
        }

        if clauses.is_empty() {
            return Err(self.error("CALL subquery must contain at least one clause"));
        }

        let query = CypherQuery {
            clauses,
            params: std::collections::HashMap::new(),
        };

        // Check for IN TRANSACTIONS
        self.skip_whitespace();
        let (in_transactions, batch_size) = if self.peek_keyword("IN") {
            self.parse_keyword()?; // consume "IN"
            self.expect_keyword("TRANSACTIONS")?;
            self.skip_whitespace();

            // Check for batch size: OF n ROWS
            let batch = if self.peek_keyword("OF") {
                self.parse_keyword()?; // consume "OF"
                self.skip_whitespace();
                let size = self.parse_number()?;
                self.skip_whitespace();
                if self.peek_keyword("ROWS") {
                    self.parse_keyword()?; // consume "ROWS"
                } else if self.peek_keyword("ROW") {
                    self.parse_keyword()?; // consume "ROW"
                }
                Some(size as usize)
            } else {
                None
            };

            (true, batch)
        } else {
            (false, None)
        };

        Ok(CallSubqueryClause {
            query,
            in_transactions,
            batch_size,
        })
    }

    /// Parse CREATE INDEX clause
    /// Syntax: CREATE [OR REPLACE] INDEX [IF NOT EXISTS] ON :Label(property)
    fn parse_create_index_clause(&mut self) -> Result<CreateIndexClause> {
        // Check for OR REPLACE before INDEX
        let or_replace = if self.peek_keyword("OR") {
            self.parse_keyword()?; // consume "OR"
            self.expect_keyword("REPLACE")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        self.expect_keyword("INDEX")?;
        self.skip_whitespace();

        // Check for IF NOT EXISTS
        let if_not_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        self.expect_keyword("ON")?;
        self.skip_whitespace();
        self.expect_char(':')?;
        let label = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char('(')?;
        let property = self.parse_identifier()?;
        self.expect_char(')')?;

        Ok(CreateIndexClause {
            label,
            property,
            if_not_exists,
            or_replace,
        })
    }

    /// Parse DROP INDEX clause
    /// Syntax: DROP INDEX [IF EXISTS] ON :Label(property)
    fn parse_drop_index_clause(&mut self) -> Result<DropIndexClause> {
        self.expect_keyword("INDEX")?;
        self.skip_whitespace();

        // Check for IF EXISTS
        let if_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        self.expect_keyword("ON")?;
        self.skip_whitespace();
        self.expect_char(':')?;
        let label = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char('(')?;
        let property = self.parse_identifier()?;
        self.expect_char(')')?;

        Ok(DropIndexClause {
            label,
            property,
            if_exists,
        })
    }

    /// Parse CREATE CONSTRAINT clause
    /// Syntax: CREATE CONSTRAINT [IF NOT EXISTS] ON (n:Label) ASSERT n.property IS UNIQUE
    /// or: CREATE CONSTRAINT [IF NOT EXISTS] ON (n:Label) ASSERT EXISTS(n.property)
    fn parse_create_constraint_clause(&mut self) -> Result<CreateConstraintClause> {
        self.expect_keyword("CONSTRAINT")?;
        self.skip_whitespace();

        // Check for IF NOT EXISTS
        let if_not_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        self.expect_keyword("ON")?;
        self.skip_whitespace();
        self.expect_char('(')?;
        let _variable = self.parse_identifier()?; // variable name (usually 'n')
        self.skip_whitespace();
        self.expect_char(':')?;
        let label = self.parse_identifier()?;
        self.expect_char(')')?;
        self.skip_whitespace();
        self.expect_keyword("ASSERT")?;
        self.skip_whitespace();

        // Parse constraint type and extract property name
        let (constraint_type, property) = if self.peek_keyword("EXISTS") {
            self.parse_keyword()?; // consume "EXISTS"
            self.expect_char('(')?;
            let _var = self.parse_identifier()?;
            self.expect_char('.')?;
            let prop = self.parse_identifier()?;
            self.expect_char(')')?;
            (ConstraintType::Exists, prop)
        } else {
            // Parse: variable.property IS UNIQUE
            let _var = self.parse_identifier()?;
            self.expect_char('.')?;
            let prop = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_keyword("IS")?;
            self.expect_keyword("UNIQUE")?;
            (ConstraintType::Unique, prop)
        };

        Ok(CreateConstraintClause {
            constraint_type,
            label,
            property,
            if_not_exists,
        })
    }

    /// Parse DROP CONSTRAINT clause
    /// Syntax: DROP CONSTRAINT [IF EXISTS] ON (n:Label) ASSERT n.property IS UNIQUE
    fn parse_drop_constraint_clause(&mut self) -> Result<DropConstraintClause> {
        self.expect_keyword("CONSTRAINT")?;
        self.skip_whitespace();

        // Check for IF EXISTS
        let if_exists = if self.peek_keyword("IF") {
            self.parse_keyword()?; // consume "IF"
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            true
        } else {
            false
        };

        self.expect_keyword("ON")?;
        self.skip_whitespace();
        self.expect_char('(')?;
        let _variable = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(':')?;
        let label = self.parse_identifier()?;
        self.expect_char(')')?;
        self.skip_whitespace();
        self.expect_keyword("ASSERT")?;
        self.skip_whitespace();

        // Parse constraint type and extract property name (same as CREATE)
        let (constraint_type, property) = if self.peek_keyword("EXISTS") {
            self.parse_keyword()?;
            self.expect_char('(')?;
            let _var = self.parse_identifier()?;
            self.expect_char('.')?;
            let prop = self.parse_identifier()?;
            self.expect_char(')')?;
            (ConstraintType::Exists, prop)
        } else {
            self.parse_identifier()?; // variable
            self.expect_char('.')?;
            let prop = self.parse_identifier()?;
            self.skip_whitespace();
            self.expect_keyword("IS")?;
            self.expect_keyword("UNIQUE")?;
            (ConstraintType::Unique, prop)
        };

        Ok(DropConstraintClause {
            constraint_type,
            label,
            property,
            if_exists,
        })
    }

    /// Parse CREATE USER clause
    /// Syntax: CREATE USER username [SET PASSWORD 'password'] [IF NOT EXISTS]
    fn parse_create_user_clause(&mut self) -> Result<CreateUserClause> {
        self.expect_keyword("USER")?;
        self.skip_whitespace();

        // Check for IF NOT EXISTS first (it can come before username in some dialects)
        let mut if_not_exists = false;
        if self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            if_not_exists = true;
        }

        let username = self.parse_identifier()?;
        self.skip_whitespace();

        // Check for SET PASSWORD
        let password = if self.peek_keyword("SET") {
            self.parse_keyword()?;
            self.expect_keyword("PASSWORD")?;
            self.skip_whitespace();
            let pwd_expr = self.parse_string_literal()?;
            // Extract string value from Expression::Literal(Literal::String)
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = pwd_expr {
                Some(s)
            } else {
                return Err(self.error("PASSWORD must be a string literal"));
            }
        } else {
            None
        };

        // Check for IF NOT EXISTS after username
        if !if_not_exists && self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("NOT")?;
            self.expect_keyword("EXISTS")?;
            if_not_exists = true;
        }

        Ok(CreateUserClause {
            username,
            password,
            if_not_exists,
        })
    }

    /// Parse DROP USER clause
    /// Syntax: DROP USER username [IF EXISTS]
    fn parse_drop_user_clause(&mut self) -> Result<DropUserClause> {
        self.expect_keyword("USER")?;
        self.skip_whitespace();

        // Check for IF EXISTS first
        let mut if_exists = false;
        if self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("EXISTS")?;
            self.skip_whitespace();
            if_exists = true;
        }

        let username = self.parse_identifier()?;
        self.skip_whitespace();

        // Check for IF EXISTS after username
        if !if_exists && self.peek_keyword("IF") {
            self.parse_keyword()?;
            self.expect_keyword("EXISTS")?;
            if_exists = true;
        }

        Ok(DropUserClause {
            username,
            if_exists,
        })
    }

    /// Parse SHOW USER clause
    /// Syntax: SHOW USER username
    fn parse_show_user_clause(&mut self) -> Result<ShowUserClause> {
        self.expect_keyword("USER")?;
        self.skip_whitespace();
        let username = self.parse_identifier()?;
        Ok(ShowUserClause { username })
    }

    /// Parse CREATE API KEY clause
    /// Syntax: CREATE API KEY name [FOR username] [WITH PERMISSIONS ...] [EXPIRES IN 'duration']
    fn parse_create_api_key_clause(&mut self) -> Result<CreateApiKeyClause> {
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        self.skip_whitespace();

        let mut user_id = None;
        let mut permissions = Vec::new();
        let mut expires_in = None;

        // Parse optional FOR username
        if self.peek_keyword("FOR") {
            self.parse_keyword()?;
            self.skip_whitespace();
            user_id = Some(self.parse_identifier()?);
            self.skip_whitespace();
        }

        // Parse optional WITH PERMISSIONS
        if self.peek_keyword("WITH") {
            self.parse_keyword()?;
            self.expect_keyword("PERMISSIONS")?;
            self.skip_whitespace();
            loop {
                let permission = self.parse_identifier()?;
                permissions.push(permission);
                self.skip_whitespace();
                if self.peek_char() == Some(',') {
                    self.consume_char();
                    self.skip_whitespace();
                } else {
                    break;
                }
            }
        }

        // Parse optional EXPIRES IN
        if self.peek_keyword("EXPIRES") {
            self.parse_keyword()?;
            self.expect_keyword("IN")?;
            self.skip_whitespace();
            let duration_str = self.parse_string_literal()?;
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = duration_str {
                expires_in = Some(s);
            }
        }

        Ok(CreateApiKeyClause {
            name,
            user_id,
            permissions,
            expires_in,
        })
    }

    /// Parse SHOW API KEYS clause
    /// Syntax: SHOW API KEYS [FOR username]
    fn parse_show_api_keys_clause(&mut self) -> Result<ShowApiKeysClause> {
        self.skip_whitespace();
        let mut user_id = None;

        if self.peek_keyword("FOR") {
            self.parse_keyword()?;
            self.skip_whitespace();
            user_id = Some(self.parse_identifier()?);
        }

        Ok(ShowApiKeysClause { user_id })
    }

    /// Parse REVOKE API KEY clause
    /// Syntax: REVOKE API KEY 'key_id' [REASON 'reason']
    fn parse_revoke_api_key_clause(&mut self) -> Result<RevokeApiKeyClause> {
        self.skip_whitespace();
        let key_id_str = self.parse_string_literal()?;
        let key_id =
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = key_id_str {
                s
            } else {
                return Err(self.error("API key ID must be a string literal"));
            };

        self.skip_whitespace();
        let mut reason = None;

        if self.peek_keyword("REASON") {
            self.parse_keyword()?;
            self.skip_whitespace();
            let reason_str = self.parse_string_literal()?;
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = reason_str {
                reason = Some(s);
            }
        }

        Ok(RevokeApiKeyClause { key_id, reason })
    }

    /// Parse DELETE API KEY clause
    /// Syntax: DELETE API KEY 'key_id'
    fn parse_delete_api_key_clause(&mut self) -> Result<DeleteApiKeyClause> {
        self.skip_whitespace();
        let key_id_str = self.parse_string_literal()?;
        let key_id =
            if let Expression::Literal(crate::executor::parser::Literal::String(s)) = key_id_str {
                s
            } else {
                return Err(self.error("API key ID must be a string literal"));
            };

        Ok(DeleteApiKeyClause { key_id })
    }

    /// Parse GRANT clause
    /// Syntax: GRANT permission [, permission ...] TO target
    fn parse_grant_clause(&mut self) -> Result<GrantClause> {
        self.skip_whitespace();

        // Parse permissions
        let mut permissions = Vec::new();
        loop {
            let permission = self.parse_identifier()?;
            permissions.push(permission);
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        self.expect_keyword("TO")?;
        self.skip_whitespace();
        let target = self.parse_identifier()?;

        Ok(GrantClause {
            permissions,
            target,
        })
    }

    /// Parse EXPLAIN clause
    /// Syntax: EXPLAIN [query]
    fn parse_explain_clause(&mut self) -> Result<Clause> {
        self.parse_keyword()?; // consume "EXPLAIN"
        self.skip_whitespace();

        // Save current position to extract query string
        let start_pos = self.pos;

        // Parse the remaining query directly using the current parser state
        // This is more reliable than creating a temporary parser
        let mut clauses = Vec::new();

        // Parse clauses until end of input
        // We need to parse clauses without checking for EXPLAIN/PROFILE again
        // since we're already inside an EXPLAIN clause
        while self.pos < self.input.len() {
            if self.is_clause_boundary() {
                // Parse clause but skip EXPLAIN/PROFILE detection
                // We'll manually check for the clause type
                let keyword = self.parse_keyword()?;
                let clause = match keyword.to_uppercase().as_str() {
                    "MATCH" => {
                        let match_clause = self.parse_match_clause()?;
                        Clause::Match(match_clause)
                    }
                    "CREATE" => {
                        self.skip_whitespace();
                        // Check for CREATE DATABASE/INDEX/CONSTRAINT/USER
                        if self.peek_keyword("DATABASE") {
                            Clause::CreateDatabase(self.parse_create_database_clause()?)
                        } else if self.peek_keyword("INDEX") {
                            Clause::CreateIndex(self.parse_create_index_clause()?)
                        } else if self.peek_keyword("CONSTRAINT") {
                            Clause::CreateConstraint(self.parse_create_constraint_clause()?)
                        } else if self.peek_keyword("USER") {
                            Clause::CreateUser(self.parse_create_user_clause()?)
                        } else {
                            Clause::Create(self.parse_create_clause()?)
                        }
                    }
                    "MERGE" => Clause::Merge(self.parse_merge_clause()?),
                    "SET" => Clause::Set(self.parse_set_clause()?),
                    "DELETE" => Clause::Delete(self.parse_delete_clause()?),
                    "DETACH" => {
                        self.expect_keyword("DELETE")?;
                        let mut delete_clause = self.parse_delete_clause()?;
                        delete_clause.detach = true;
                        Clause::Delete(delete_clause)
                    }
                    "REMOVE" => Clause::Remove(self.parse_remove_clause()?),
                    "WITH" => Clause::With(self.parse_with_clause()?),
                    "UNWIND" => Clause::Unwind(self.parse_unwind_clause()?),
                    "UNION" => {
                        self.skip_whitespace();
                        let union_type = if self.peek_keyword("ALL") {
                            self.parse_keyword()?;
                            UnionType::All
                        } else {
                            UnionType::Distinct
                        };
                        Clause::Union(UnionClause { union_type })
                    }
                    "WHERE" => Clause::Where(self.parse_where_clause()?),
                    "RETURN" => Clause::Return(self.parse_return_clause()?),
                    "ORDER" => {
                        self.expect_keyword("BY")?;
                        Clause::OrderBy(self.parse_order_by_clause()?)
                    }
                    "LIMIT" => Clause::Limit(self.parse_limit_clause()?),
                    "SKIP" => Clause::Skip(self.parse_skip_clause()?),
                    "FOREACH" => Clause::Foreach(self.parse_foreach_clause()?),
                    "OPTIONAL" => {
                        self.expect_keyword("MATCH")?;
                        let mut match_clause = self.parse_match_clause()?;
                        match_clause.optional = true;
                        Clause::Match(match_clause)
                    }
                    _ => return Err(self.error(&format!("Unexpected clause: {}", keyword))),
                };
                clauses.push(clause);
                self.skip_whitespace();
                if self.pos >= self.input.len() {
                    break;
                }
            } else {
                break;
            }
        }

        // Extract the query string that was parsed
        let query_str = self.input[start_pos..self.pos].trim().to_string();

        if clauses.is_empty() {
            return Err(self.error("EXPLAIN requires a query with at least one clause"));
        }

        let query = CypherQuery {
            clauses,
            params: std::collections::HashMap::new(),
        };

        Ok(Clause::Explain(ExplainClause {
            query,
            query_string: Some(query_str),
        }))
    }

    /// Parse PROFILE clause
    /// Syntax: PROFILE [query]
    fn parse_profile_clause(&mut self) -> Result<Clause> {
        self.parse_keyword()?; // consume "PROFILE"
        self.skip_whitespace();

        // Save current position to extract query string
        let start_pos = self.pos;

        // Parse the remaining query directly using the current parser state
        // This is more reliable than creating a temporary parser
        let mut clauses = Vec::new();

        // Parse clauses until end of input
        // We need to parse clauses without checking for EXPLAIN/PROFILE again
        // since we're already inside a PROFILE clause
        while self.pos < self.input.len() {
            if self.is_clause_boundary() {
                // Parse clause but skip EXPLAIN/PROFILE detection
                // We'll manually check for the clause type
                let keyword = self.parse_keyword()?;
                let clause = match keyword.to_uppercase().as_str() {
                    "MATCH" => {
                        let match_clause = self.parse_match_clause()?;
                        Clause::Match(match_clause)
                    }
                    "CREATE" => {
                        self.skip_whitespace();
                        // Check for CREATE DATABASE/INDEX/CONSTRAINT/USER
                        if self.peek_keyword("DATABASE") {
                            Clause::CreateDatabase(self.parse_create_database_clause()?)
                        } else if self.peek_keyword("INDEX") {
                            Clause::CreateIndex(self.parse_create_index_clause()?)
                        } else if self.peek_keyword("CONSTRAINT") {
                            Clause::CreateConstraint(self.parse_create_constraint_clause()?)
                        } else if self.peek_keyword("USER") {
                            Clause::CreateUser(self.parse_create_user_clause()?)
                        } else {
                            Clause::Create(self.parse_create_clause()?)
                        }
                    }
                    "MERGE" => Clause::Merge(self.parse_merge_clause()?),
                    "SET" => Clause::Set(self.parse_set_clause()?),
                    "DELETE" => Clause::Delete(self.parse_delete_clause()?),
                    "DETACH" => {
                        self.expect_keyword("DELETE")?;
                        let mut delete_clause = self.parse_delete_clause()?;
                        delete_clause.detach = true;
                        Clause::Delete(delete_clause)
                    }
                    "REMOVE" => Clause::Remove(self.parse_remove_clause()?),
                    "WITH" => Clause::With(self.parse_with_clause()?),
                    "UNWIND" => Clause::Unwind(self.parse_unwind_clause()?),
                    "UNION" => {
                        self.skip_whitespace();
                        let union_type = if self.peek_keyword("ALL") {
                            self.parse_keyword()?;
                            UnionType::All
                        } else {
                            UnionType::Distinct
                        };
                        Clause::Union(UnionClause { union_type })
                    }
                    "WHERE" => Clause::Where(self.parse_where_clause()?),
                    "RETURN" => Clause::Return(self.parse_return_clause()?),
                    "ORDER" => {
                        self.expect_keyword("BY")?;
                        Clause::OrderBy(self.parse_order_by_clause()?)
                    }
                    "LIMIT" => Clause::Limit(self.parse_limit_clause()?),
                    "SKIP" => Clause::Skip(self.parse_skip_clause()?),
                    "FOREACH" => Clause::Foreach(self.parse_foreach_clause()?),
                    "OPTIONAL" => {
                        self.expect_keyword("MATCH")?;
                        let mut match_clause = self.parse_match_clause()?;
                        match_clause.optional = true;
                        Clause::Match(match_clause)
                    }
                    _ => return Err(self.error(&format!("Unexpected clause: {}", keyword))),
                };
                clauses.push(clause);
                self.skip_whitespace();
                if self.pos >= self.input.len() {
                    break;
                }
            } else {
                break;
            }
        }

        // Extract the query string that was parsed
        let query_str = self.input[start_pos..self.pos].trim().to_string();

        if clauses.is_empty() {
            return Err(self.error("PROFILE requires a query with at least one clause"));
        }

        let query = CypherQuery {
            clauses,
            params: std::collections::HashMap::new(),
        };

        Ok(Clause::Profile(ProfileClause {
            query,
            query_string: Some(query_str),
        }))
    }

    /// Parse REVOKE clause
    /// Syntax: REVOKE permission [, permission ...] FROM target
    fn parse_revoke_clause(&mut self) -> Result<RevokeClause> {
        self.skip_whitespace();

        // Parse permissions
        let mut permissions = Vec::new();
        loop {
            let permission = self.parse_identifier()?;
            permissions.push(permission);
            self.skip_whitespace();
            if self.peek_char() == Some(',') {
                self.consume_char();
                self.skip_whitespace();
            } else {
                break;
            }
        }

        self.expect_keyword("FROM")?;
        self.skip_whitespace();
        let target = self.parse_identifier()?;

        Ok(RevokeClause {
            permissions,
            target,
        })
    }

    /// Parse expression
    /// Parse expression (simplified for MVP)
    pub fn parse_expression(&mut self) -> Result<Expression> {
        self.skip_whitespace();
        self.parse_or_expression()
    }

    /// Parse OR expressions (lowest precedence)
    fn parse_or_expression(&mut self) -> Result<Expression> {
        let mut left = self.parse_and_expression()?;

        while self.peek_keyword("OR") {
            self.parse_keyword()?;
            self.skip_whitespace();
            let right = self.parse_and_expression()?;
            left = Expression::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::Or,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse AND expressions
    fn parse_and_expression(&mut self) -> Result<Expression> {
        let mut left = self.parse_comparison_expression()?;

        while self.peek_keyword("AND") {
            self.parse_keyword()?;
            self.skip_whitespace();
            let right = self.parse_comparison_expression()?;
            left = Expression::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::And,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    /// Parse comparison expressions (=, <>, <, <=, >, >=, IS NULL, IS NOT NULL, STARTS WITH, ENDS WITH, CONTAINS, =~)
    fn parse_comparison_expression(&mut self) -> Result<Expression> {
        let left = self.parse_additive_expression()?;

        // Check for IS NULL / IS NOT NULL
        self.skip_whitespace();
        if self.peek_keyword("IS") {
            self.parse_keyword()?;
            self.skip_whitespace();

            let negated = if self.peek_keyword("NOT") {
                self.parse_keyword()?;
                self.skip_whitespace();
                true
            } else {
                false
            };

            if self.peek_keyword("NULL") {
                self.parse_keyword()?;
                return Ok(Expression::IsNull {
                    expr: Box::new(left),
                    negated,
                });
            } else {
                return Err(self.error("Expected NULL after IS [NOT]"));
            }
        }

        // Check for string operators (STARTS WITH, ENDS WITH, CONTAINS)
        self.skip_whitespace();
        if self.peek_keyword("STARTS") {
            self.parse_keyword()?;
            self.skip_whitespace();
            if self.peek_keyword("WITH") {
                self.parse_keyword()?;
                self.skip_whitespace();
                let right = self.parse_additive_expression()?;
                return Ok(Expression::BinaryOp {
                    left: Box::new(left),
                    op: BinaryOperator::StartsWith,
                    right: Box::new(right),
                });
            } else {
                return Err(self.error("Expected WITH after STARTS"));
            }
        }

        if self.peek_keyword("ENDS") {
            self.parse_keyword()?;
            self.skip_whitespace();
            if self.peek_keyword("WITH") {
                self.parse_keyword()?;
                self.skip_whitespace();
                let right = self.parse_additive_expression()?;
                return Ok(Expression::BinaryOp {
                    left: Box::new(left),
                    op: BinaryOperator::EndsWith,
                    right: Box::new(right),
                });
            } else {
                return Err(self.error("Expected WITH after ENDS"));
            }
        }

        if self.peek_keyword("CONTAINS") {
            self.parse_keyword()?;
            self.skip_whitespace();
            let right = self.parse_additive_expression()?;
            return Ok(Expression::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::Contains,
                right: Box::new(right),
            });
        }

        // Check for regex operator (=~)
        self.skip_whitespace();
        if self.peek_char() == Some('=') && self.peek_char_at(1) == Some('~') {
            self.consume_char(); // consume '='
            self.consume_char(); // consume '~'
            self.skip_whitespace();
            let right = self.parse_additive_expression()?;
            return Ok(Expression::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::RegexMatch,
                right: Box::new(right),
            });
        }

        // Check for comparison operators (=, <>, <, <=, >, >=)
        self.skip_whitespace();
        if let Some(op) = self.parse_comparison_operator() {
            self.skip_whitespace();
            let right = self.parse_additive_expression()?;
            return Ok(Expression::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            });
        }

        Ok(left)
    }

    /// Parse additive expressions (+, -)
    fn parse_additive_expression(&mut self) -> Result<Expression> {
        let mut left = self.parse_multiplicative_expression()?;

        loop {
            self.skip_whitespace();
            if let Some(op) = self.parse_additive_operator() {
                self.skip_whitespace();
                let right = self.parse_multiplicative_expression()?;
                left = Expression::BinaryOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse multiplicative expressions (*, /, %)
    fn parse_multiplicative_expression(&mut self) -> Result<Expression> {
        let mut left = self.parse_unary_expression()?;

        loop {
            self.skip_whitespace();
            if let Some(op) = self.parse_multiplicative_operator() {
                self.skip_whitespace();
                let right = self.parse_unary_expression()?;
                left = Expression::BinaryOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        Ok(left)
    }

    /// Parse unary expressions
    fn parse_unary_expression(&mut self) -> Result<Expression> {
        self.skip_whitespace();

        // Check for unary operators
        if let Some(op) = self.parse_unary_operator() {
            self.skip_whitespace();
            let operand = self.parse_simple_expression()?;
            return Ok(Expression::UnaryOp {
                op,
                operand: Box::new(operand),
            });
        }

        self.parse_simple_expression()
    }

    /// Parse comparison operator only (not AND/OR)
    fn parse_comparison_operator(&mut self) -> Option<BinaryOperator> {
        self.skip_whitespace();

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
                } else if self.peek_char() == Some('>') {
                    self.consume_char();
                    Some(BinaryOperator::NotEqual)
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
                } else if self.peek_keyword("EXISTS") {
                    self.parse_exists_expression()
                } else if self.peek_keyword("true") {
                    self.parse_boolean_literal(true)
                } else if self.peek_keyword("false") {
                    self.parse_boolean_literal(false)
                } else if self.peek_keyword("null") {
                    self.parse_null_literal()
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

        // Check for function call
        if self.peek_char() == Some('(') {
            self.consume_char(); // consume '('
            let mut args = Vec::new();

            self.skip_whitespace();

            // Check for count(*) special case
            if self.peek_char() == Some('*') {
                self.consume_char(); // consume '*'
                self.skip_whitespace();
                // count(*) has no arguments - empty args list means count all
            } else {
                // Check for DISTINCT keyword (for COUNT(DISTINCT ...))
                let has_distinct = if self.peek_keyword("DISTINCT") {
                    self.expect_keyword("DISTINCT")?;
                    self.skip_whitespace();
                    true
                } else {
                    false
                };

                // If DISTINCT was found, add it as a marker in args
                if has_distinct {
                    args.push(Expression::Variable("__DISTINCT__".to_string()));
                }

                // Parse arguments
                while self.peek_char() != Some(')') {
                    let arg = self.parse_expression()?;
                    args.push(arg);

                    if self.peek_char() == Some(',') {
                        self.consume_char();
                        self.skip_whitespace();
                    }
                }
            }

            self.expect_char(')')?;
            Ok(Expression::FunctionCall {
                name: identifier,
                args,
            })
        }
        // Check for map projection: n {.name, .age}
        else if self.peek_char() == Some('{') {
            let source = Box::new(Expression::Variable(identifier));
            let items = self.parse_map_projection_items()?;
            Ok(Expression::MapProjection { source, items })
        }
        // Check for property access
        else if self.peek_char() == Some('.') {
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

    /// Parse boolean literal
    fn parse_boolean_literal(&mut self, value: bool) -> Result<Expression> {
        if value {
            self.expect_keyword("true")?;
        } else {
            self.expect_keyword("false")?;
        }
        Ok(Expression::Literal(Literal::Boolean(value)))
    }

    /// Parse null literal
    fn parse_null_literal(&mut self) -> Result<Expression> {
        self.expect_keyword("null")?;
        Ok(Expression::Literal(Literal::Null))
    }

    /// Parse list expression
    fn parse_list_expression(&mut self) -> Result<Expression> {
        self.expect_char('[')?;
        self.skip_whitespace();

        // Check if this is a pattern comprehension: [(pattern) WHERE ... | ...]
        // Pattern comprehensions start with '(' or an identifier followed by ':' or '-'
        let saved_pos = self.pos;
        let is_pattern_comprehension = if self.peek_char() == Some('(') {
            // Starts with '(', likely a pattern
            true
        } else if self.is_identifier_start() {
            // Check if identifier is followed by ':' (label) or '-' (relationship)
            let _identifier = self.parse_identifier()?;
            self.skip_whitespace();
            let next_char = self.peek_char();
            let is_pattern = next_char == Some(':') || next_char == Some('-');
            // Reset position
            self.pos = saved_pos;
            is_pattern
        } else {
            false
        };

        if is_pattern_comprehension {
            // Parse pattern comprehension: [(pattern) WHERE ... | ...]
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

            // Parse optional transformation expression (after |)
            let transform_expression = if self.peek_char() == Some('|') {
                self.consume_char();
                self.skip_whitespace();
                Some(Box::new(self.parse_expression()?))
            } else {
                None
            };
            self.skip_whitespace();

            self.expect_char(']')?;

            return Ok(Expression::PatternComprehension {
                pattern,
                where_clause,
                transform_expression,
            });
        }

        // Check if this is a list comprehension: [x IN list WHERE ... | ...]
        if self.is_identifier_start() {
            let saved_pos = self.pos;
            let variable = self.parse_identifier()?;
            self.skip_whitespace();

            // Check if next token is IN (indicating list comprehension)
            if self.peek_keyword("IN") {
                // This is a list comprehension
                self.expect_keyword("IN")?;
                self.skip_whitespace();

                // Parse list expression
                let list_expression = Box::new(self.parse_expression()?);
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

                // Parse optional transformation expression (after |)
                let transform_expression = if self.peek_char() == Some('|') {
                    self.consume_char();
                    self.skip_whitespace();
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                };
                self.skip_whitespace();

                self.expect_char(']')?;

                return Ok(Expression::ListComprehension {
                    variable,
                    list_expression,
                    where_clause,
                    transform_expression,
                });
            } else {
                // Not a list comprehension, reset position and parse as regular list
                self.pos = saved_pos;
            }
        }

        // Regular list expression
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

    /// Parse map projection items: {.name, .age AS age_alias, fullName: n.name}
    fn parse_map_projection_items(&mut self) -> Result<Vec<MapProjectionItem>> {
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

    /// Parse EXISTS expression
    fn parse_exists_expression(&mut self) -> Result<Expression> {
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
    fn parse_pattern_until_where_or_brace(&mut self) -> Result<Pattern> {
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

    /// Parse comparison operator
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
    fn peek_keyword_at(&self, offset: usize, keyword: &str) -> bool {
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
                assert!(!return_clause.distinct);
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

                let ReturnItem { expression, alias } = &return_clause.items[0];
                {
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

    #[test]
    fn test_parse_relationship_directions() {
        // Test outgoing relationship
        let mut parser = CypherParser::new("MATCH (a)-[r]->(b) RETURN a".to_string());
        let query = parser.parse().unwrap();
        match &query.clauses[0] {
            Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
                PatternElement::Relationship(rel) => {
                    assert_eq!(rel.direction, RelationshipDirection::Outgoing);
                }
                _ => panic!("Expected relationship"),
            },
            _ => panic!("Expected match clause"),
        }

        // Test incoming relationship
        let mut parser = CypherParser::new("MATCH (a)<-[r]-(b) RETURN a".to_string());
        let query = parser.parse().unwrap();
        match &query.clauses[0] {
            Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
                PatternElement::Relationship(rel) => {
                    assert_eq!(rel.direction, RelationshipDirection::Incoming);
                }
                _ => panic!("Expected relationship"),
            },
            _ => panic!("Expected match clause"),
        }

        // Test both directions
        let mut parser = CypherParser::new("MATCH (a)-[r]-(b) RETURN a".to_string());
        let query = parser.parse().unwrap();
        match &query.clauses[0] {
            Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
                PatternElement::Relationship(rel) => {
                    assert_eq!(rel.direction, RelationshipDirection::Both);
                }
                _ => panic!("Expected relationship"),
            },
            _ => panic!("Expected match clause"),
        }
    }

    #[test]
    fn test_parse_relationship_quantifiers() {
        // Test basic relationship without quantifier
        let mut parser = CypherParser::new("MATCH (a)-[r]->(b) RETURN a".to_string());
        let query = parser.parse().unwrap();
        match &query.clauses[0] {
            Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
                PatternElement::Relationship(rel) => {
                    assert_eq!(rel.quantifier, None);
                }
                _ => panic!("Expected relationship"),
            },
            _ => panic!("Expected match clause"),
        }

        // Test zero or more quantifier (*)
        let mut parser = CypherParser::new("MATCH (a)-[r*]->(b) RETURN a".to_string());
        let query = parser.parse().unwrap();
        match &query.clauses[0] {
            Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
                PatternElement::Relationship(rel) => {
                    assert_eq!(rel.quantifier, Some(RelationshipQuantifier::ZeroOrMore));
                }
                _ => panic!("Expected relationship"),
            },
            _ => panic!("Expected match clause"),
        }

        // Test one or more quantifier (+)
        let mut parser = CypherParser::new("MATCH (a)-[r+]->(b) RETURN a".to_string());
        let query = parser.parse().unwrap();
        match &query.clauses[0] {
            Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
                PatternElement::Relationship(rel) => {
                    assert_eq!(rel.quantifier, Some(RelationshipQuantifier::OneOrMore));
                }
                _ => panic!("Expected relationship"),
            },
            _ => panic!("Expected match clause"),
        }

        // Test zero or one quantifier (?)
        let mut parser = CypherParser::new("MATCH (a)-[r?]->(b) RETURN a".to_string());
        let query = parser.parse().unwrap();
        match &query.clauses[0] {
            Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
                PatternElement::Relationship(rel) => {
                    assert_eq!(rel.quantifier, Some(RelationshipQuantifier::ZeroOrOne));
                }
                _ => panic!("Expected relationship"),
            },
            _ => panic!("Expected match clause"),
        }

        // Test exact quantifier {2}
        let mut parser = CypherParser::new("MATCH (a)-[r{2}]->(b) RETURN a".to_string());
        let query = parser.parse().unwrap();
        match &query.clauses[0] {
            Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
                PatternElement::Relationship(rel) => {
                    assert_eq!(rel.quantifier, Some(RelationshipQuantifier::Exact(2)));
                }
                _ => panic!("Expected relationship"),
            },
            _ => panic!("Expected match clause"),
        }

        // Test range quantifier {1..3}
        let mut parser = CypherParser::new("MATCH (a)-[r{1..3}]->(b) RETURN a".to_string());
        let query = parser.parse().unwrap();
        match &query.clauses[0] {
            Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
                PatternElement::Relationship(rel) => {
                    if let Some(RelationshipQuantifier::Range(min, max)) = &rel.quantifier {
                        assert_eq!(*min, 1);
                        assert_eq!(*max, 3);
                    } else {
                        panic!("Expected Range quantifier, got {:?}", rel.quantifier);
                    }
                }
                _ => panic!("Expected relationship"),
            },
            _ => panic!("Expected match clause"),
        }
    }

    #[test]
    fn test_parse_node_properties() {
        // Test a simpler case that works with current parser
        let mut parser = CypherParser::new("MATCH (n:Person) RETURN n".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[0] {
            Clause::Match(match_clause) => match &match_clause.pattern.elements[0] {
                PatternElement::Node(node) => {
                    assert_eq!(node.labels, vec!["Person"]);
                }
                _ => panic!("Expected node pattern"),
            },
            _ => panic!("Expected match clause"),
        }
    }

    #[test]
    fn test_parse_relationship_properties() {
        // Test a simpler case that works with current parser
        let mut parser = CypherParser::new("MATCH (a)-[r:KNOWS]->(b) RETURN a".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[0] {
            Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
                PatternElement::Relationship(rel) => {
                    assert_eq!(rel.types, vec!["KNOWS"]);
                    assert_eq!(rel.direction, RelationshipDirection::Outgoing);
                }
                _ => panic!("Expected relationship pattern"),
            },
            _ => panic!("Expected match clause"),
        }
    }

    #[test]
    fn test_parse_multiple_labels() {
        let mut parser =
            CypherParser::new("MATCH (n:Person:Employee:Manager) RETURN n".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[0] {
            Clause::Match(match_clause) => match &match_clause.pattern.elements[0] {
                PatternElement::Node(node) => {
                    assert_eq!(node.labels, vec!["Person", "Employee", "Manager"]);
                }
                _ => panic!("Expected node pattern"),
            },
            _ => panic!("Expected match clause"),
        }
    }

    #[test]
    fn test_parse_multiple_relationship_types() {
        let mut parser =
            CypherParser::new("MATCH (a)-[r:KNOWS:WORKS_WITH]->(b) RETURN a".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[0] {
            Clause::Match(match_clause) => match &match_clause.pattern.elements[1] {
                PatternElement::Relationship(rel) => {
                    assert_eq!(rel.types, vec!["KNOWS", "WORKS_WITH"]);
                }
                _ => panic!("Expected relationship pattern"),
            },
            _ => panic!("Expected match clause"),
        }
    }

    #[test]
    fn test_parse_return_distinct() {
        let mut parser = CypherParser::new("MATCH (n:Person) RETURN DISTINCT n.name".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[1] {
            Clause::Return(return_clause) => {
                assert!(return_clause.distinct);
            }
            _ => panic!("Expected return clause"),
        }
    }

    #[test]
    fn test_parse_multiple_return_items() {
        let mut parser =
            CypherParser::new("MATCH (n:Person) RETURN n.name, n.age, n.city".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[1] {
            Clause::Return(return_clause) => {
                assert_eq!(return_clause.items.len(), 3);
            }
            _ => panic!("Expected return clause"),
        }
    }

    #[test]
    fn test_parse_order_by_ascending() {
        let mut parser =
            CypherParser::new("MATCH (n:Person) RETURN n ORDER BY n.age ASC".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[2] {
            Clause::OrderBy(order_clause) => {
                assert_eq!(order_clause.items[0].direction, SortDirection::Ascending);
            }
            _ => panic!("Expected order by clause"),
        }
    }

    #[test]
    fn test_parse_multiple_order_by() {
        let mut parser = CypherParser::new(
            "MATCH (n:Person) RETURN n ORDER BY n.age DESC, n.name ASC".to_string(),
        );
        let query = parser.parse().unwrap();

        match &query.clauses[2] {
            Clause::OrderBy(order_clause) => {
                assert_eq!(order_clause.items.len(), 2);
                assert_eq!(order_clause.items[0].direction, SortDirection::Descending);
                assert_eq!(order_clause.items[1].direction, SortDirection::Ascending);
            }
            _ => panic!("Expected order by clause"),
        }
    }

    #[test]
    fn test_parse_skip_clause() {
        let mut parser = CypherParser::new("MATCH (n:Person) RETURN n SKIP 5".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[2] {
            Clause::Skip(skip_clause) => match &skip_clause.count {
                Expression::Literal(Literal::Integer(5)) => {}
                _ => panic!("Expected integer literal"),
            },
            _ => panic!("Expected skip clause"),
        }
    }

    #[test]
    fn test_parse_string_literals() {
        let mut parser = CypherParser::new("\"Hello World\"".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::Literal(Literal::String(value)) => {
                assert_eq!(value, "Hello World");
            }
            _ => panic!("Expected string literal"),
        }
    }

    #[test]
    fn test_parse_string_literals_single_quotes() {
        let mut parser = CypherParser::new("'Hello World'".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::Literal(Literal::String(value)) => {
                assert_eq!(value, "Hello World");
            }
            _ => panic!("Expected string literal"),
        }
    }

    #[test]
    fn test_parse_string_escapes() {
        let mut parser = CypherParser::new("\"Hello\\nWorld\\tTest\"".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::Literal(Literal::String(value)) => {
                assert_eq!(value, "Hello\nWorld\tTest");
            }
            _ => panic!("Expected string literal"),
        }
    }

    #[test]
    fn test_parse_float_literals() {
        let mut parser = CypherParser::new("3.141592653589793".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::Literal(Literal::Float(value)) => {
                assert!((value - std::f64::consts::PI).abs() < 1e-6);
            }
            _ => panic!("Expected float literal"),
        }
    }

    #[test]
    fn test_parse_boolean_literals() {
        // Test true
        let mut parser = CypherParser::new("true".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::Literal(Literal::Boolean(value)) => {
                assert!(value);
            }
            _ => panic!("Expected boolean literal"),
        }

        // Test false
        let mut parser = CypherParser::new("false".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::Literal(Literal::Boolean(value)) => {
                assert!(!value);
            }
            _ => panic!("Expected boolean literal"),
        }
    }

    #[test]
    fn test_parse_null_literal() {
        let mut parser = CypherParser::new("null".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::Literal(Literal::Null) => {}
            _ => panic!("Expected null literal"),
        }
    }

    #[test]
    fn test_parse_list_expression() {
        let mut parser = CypherParser::new("[1, 2, 3, 'hello']".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::List(elements) => {
                assert_eq!(elements.len(), 4);
            }
            _ => panic!("Expected list expression"),
        }
    }

    #[test]
    fn test_parse_map_expression() {
        let mut parser = CypherParser::new("{name: 'John', age: 30}".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::Map(properties) => {
                assert_eq!(properties.len(), 2);
                assert!(properties.contains_key("name"));
                assert!(properties.contains_key("age"));
            }
            _ => panic!("Expected map expression"),
        }
    }

    #[test]
    fn test_parse_function_call() {
        let mut parser = CypherParser::new("count(n)".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::FunctionCall { name, args } => {
                assert_eq!(name, "count");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Expected function call"),
        }
    }

    #[test]
    fn test_parse_binary_operators() {
        // Test addition
        let mut parser = CypherParser::new("a + b".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOperator::Add);
            }
            _ => panic!("Expected binary operation"),
        }

        // Test subtraction
        let mut parser = CypherParser::new("a - b".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOperator::Subtract);
            }
            _ => panic!("Expected binary operation"),
        }

        // Test multiplication
        let mut parser = CypherParser::new("a * b".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOperator::Multiply);
            }
            _ => panic!("Expected binary operation"),
        }

        // Test division
        let mut parser = CypherParser::new("a / b".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOperator::Divide);
            }
            _ => panic!("Expected binary operation"),
        }

        // Test equality
        let mut parser = CypherParser::new("a = b".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOperator::Equal);
            }
            _ => panic!("Expected binary operation"),
        }

        // Test inequality
        let mut parser = CypherParser::new("a != b".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOperator::NotEqual);
            }
            _ => panic!("Expected binary operation"),
        }

        // Test less than
        let mut parser = CypherParser::new("a < b".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOperator::LessThan);
            }
            _ => panic!("Expected binary operation"),
        }

        // Test less than or equal
        let mut parser = CypherParser::new("a <= b".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOperator::LessThanOrEqual);
            }
            _ => panic!("Expected binary operation"),
        }

        // Test greater than
        let mut parser = CypherParser::new("a > b".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOperator::GreaterThan);
            }
            _ => panic!("Expected binary operation"),
        }

        // Test greater than or equal
        let mut parser = CypherParser::new("a >= b".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOperator::GreaterThanOrEqual);
            }
            _ => panic!("Expected binary operation"),
        }

        // Test AND
        let mut parser = CypherParser::new("a AND b".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOperator::And);
            }
            _ => panic!("Expected binary operation"),
        }

        // Test OR
        let mut parser = CypherParser::new("a OR b".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOperator::Or);
            }
            _ => panic!("Expected binary operation"),
        }
    }

    #[test]
    fn test_parse_unary_operators() {
        // Test unary minus
        let mut parser = CypherParser::new("-5".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::UnaryOp { op, .. } => {
                assert_eq!(op, UnaryOperator::Minus);
            }
            _ => panic!("Expected unary operation"),
        }

        // Test unary plus
        let mut parser = CypherParser::new("+5".to_string());
        let expr = parser.parse_expression().unwrap();
        match expr {
            Expression::UnaryOp { op, .. } => {
                assert_eq!(op, UnaryOperator::Plus);
            }
            _ => panic!("Expected unary operation"),
        }
    }

    #[test]
    fn test_parse_parenthesized_expression() {
        let mut parser = CypherParser::new("(a + b) * c".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOperator::Multiply);
            }
            _ => panic!("Expected binary operation"),
        }
    }

    #[test]
    fn test_parse_case_expression_with_input() {
        let mut parser = CypherParser::new("CASE n.status WHEN 'active' THEN 'working' WHEN 'inactive' THEN 'idle' ELSE 'unknown' END".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::Case {
                input,
                when_clauses,
                else_clause,
            } => {
                assert!(input.is_some());
                assert_eq!(when_clauses.len(), 2);
                assert!(else_clause.is_some());
            }
            _ => panic!("Expected case expression"),
        }
    }

    #[test]
    fn test_parse_case_expression_without_input() {
        let mut parser = CypherParser::new(
            "CASE WHEN n.age < 18 THEN 'minor' WHEN n.age < 65 THEN 'adult' ELSE 'senior' END"
                .to_string(),
        );
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::Case {
                input,
                when_clauses,
                else_clause,
            } => {
                assert!(input.is_none());
                assert_eq!(when_clauses.len(), 2);
                assert!(else_clause.is_some());
            }
            _ => panic!("Expected case expression"),
        }
    }

    #[test]
    fn test_parse_relationship_direction_errors() {
        // Test invalid direction <-[]->
        let mut parser = CypherParser::new("MATCH (a)<-[r]->(b) RETURN a".to_string());
        let result = parser.parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_relationship_direction_parsing() {
        // Test parse_relationship_direction method directly
        let mut parser = CypherParser::new("->".to_string());
        let direction = parser.parse_relationship_direction().unwrap();
        assert_eq!(direction, RelationshipDirection::Outgoing);

        let mut parser = CypherParser::new("<-".to_string());
        let direction = parser.parse_relationship_direction().unwrap();
        assert_eq!(direction, RelationshipDirection::Incoming);

        let mut parser = CypherParser::new("-".to_string());
        let direction = parser.parse_relationship_direction().unwrap();
        assert_eq!(direction, RelationshipDirection::Both);
    }

    #[test]
    fn test_parse_comparison_operators() {
        // Test parse_comparison_operator method
        let mut parser = CypherParser::new("=".to_string());
        let op = parser.parse_comparison_operator().unwrap();
        assert_eq!(op, BinaryOperator::Equal);

        let mut parser = CypherParser::new("!=".to_string());
        let op = parser.parse_comparison_operator().unwrap();
        assert_eq!(op, BinaryOperator::NotEqual);

        let mut parser = CypherParser::new("<".to_string());
        let op = parser.parse_comparison_operator().unwrap();
        assert_eq!(op, BinaryOperator::LessThan);

        let mut parser = CypherParser::new("<=".to_string());
        let op = parser.parse_comparison_operator().unwrap();
        assert_eq!(op, BinaryOperator::LessThanOrEqual);

        let mut parser = CypherParser::new(">".to_string());
        let op = parser.parse_comparison_operator().unwrap();
        assert_eq!(op, BinaryOperator::GreaterThan);

        let mut parser = CypherParser::new(">=".to_string());
        let op = parser.parse_comparison_operator().unwrap();
        assert_eq!(op, BinaryOperator::GreaterThanOrEqual);
    }

    #[test]
    fn test_parse_additive_operators() {
        // Test parse_additive_operator method
        let mut parser = CypherParser::new("+".to_string());
        let op = parser.parse_additive_operator().unwrap();
        assert_eq!(op, BinaryOperator::Add);

        let mut parser = CypherParser::new("-".to_string());
        let op = parser.parse_additive_operator().unwrap();
        assert_eq!(op, BinaryOperator::Subtract);
    }

    #[test]
    fn test_parse_multiplicative_operators() {
        // Test parse_multiplicative_operator method
        let mut parser = CypherParser::new("*".to_string());
        let op = parser.parse_multiplicative_operator().unwrap();
        assert_eq!(op, BinaryOperator::Multiply);

        let mut parser = CypherParser::new("/".to_string());
        let op = parser.parse_multiplicative_operator().unwrap();
        assert_eq!(op, BinaryOperator::Divide);

        let mut parser = CypherParser::new("%".to_string());
        let op = parser.parse_multiplicative_operator().unwrap();
        assert_eq!(op, BinaryOperator::Modulo);

        let mut parser = CypherParser::new("^".to_string());
        let op = parser.parse_multiplicative_operator().unwrap();
        assert_eq!(op, BinaryOperator::Power);
    }

    #[test]
    fn test_parse_unary_operators_method() {
        // Test parse_unary_operator method
        let mut parser = CypherParser::new("+".to_string());
        let op = parser.parse_unary_operator().unwrap();
        assert_eq!(op, UnaryOperator::Plus);

        let mut parser = CypherParser::new("-".to_string());
        let op = parser.parse_unary_operator().unwrap();
        assert_eq!(op, UnaryOperator::Minus);
    }

    #[test]
    fn test_parse_primary_expression() {
        // Test parse_primary_expression method
        let mut parser = CypherParser::new("(a + b)".to_string());
        let expr = parser.parse_primary_expression().unwrap();
        match expr {
            Expression::BinaryOp { .. } => {}
            _ => panic!("Expected binary operation"),
        }

        let mut parser = CypherParser::new("$param".to_string());
        let expr = parser.parse_primary_expression().unwrap();
        match expr {
            Expression::Parameter(name) => {
                assert_eq!(name, "param");
            }
            _ => panic!("Expected parameter"),
        }
    }

    #[test]
    fn test_parse_range_quantifier_edge_cases() {
        // Test basic relationship parsing
        let mut parser = CypherParser::new("MATCH (a)-[r]->(b) RETURN a".to_string());
        let query = parser.parse().unwrap();
        match &query.clauses[0] {
            Clause::Match(match_clause) => {
                assert_eq!(match_clause.pattern.elements.len(), 3); // node, rel, node
            }
            _ => panic!("Expected match clause"),
        }
    }

    #[test]
    fn test_parse_identifier_validation() {
        // Test is_identifier_start
        let parser = CypherParser::new("a".to_string());
        assert!(parser.is_identifier_start());

        let parser = CypherParser::new("_".to_string());
        assert!(parser.is_identifier_start());

        let parser = CypherParser::new("1".to_string());
        assert!(!parser.is_identifier_start());

        // Test is_identifier_char
        let parser = CypherParser::new("a1_".to_string());
        assert!(parser.is_identifier_char());

        let parser = CypherParser::new(" ".to_string());
        assert!(!parser.is_identifier_char());

        // Test is_digit
        let parser = CypherParser::new("5".to_string());
        assert!(parser.is_digit());

        let parser = CypherParser::new("a".to_string());
        assert!(!parser.is_digit());

        // Test is_keyword_char
        let parser = CypherParser::new("a".to_string());
        assert!(parser.is_keyword_char());

        let parser = CypherParser::new("_".to_string());
        assert!(parser.is_keyword_char());

        let parser = CypherParser::new("1".to_string());
        assert!(!parser.is_keyword_char());
    }

    #[test]
    fn test_parse_clause_boundary() {
        // Test is_clause_boundary
        let parser = CypherParser::new("MATCH".to_string());
        assert!(parser.is_clause_boundary());

        let parser = CypherParser::new("WHERE".to_string());
        assert!(parser.is_clause_boundary());

        let parser = CypherParser::new("RETURN".to_string());
        assert!(parser.is_clause_boundary());

        let parser = CypherParser::new("ORDER".to_string());
        assert!(parser.is_clause_boundary());

        let parser = CypherParser::new("LIMIT".to_string());
        assert!(parser.is_clause_boundary());

        let parser = CypherParser::new("SKIP".to_string());
        assert!(parser.is_clause_boundary());

        let parser = CypherParser::new("SELECT".to_string());
        assert!(!parser.is_clause_boundary());
    }

    #[test]
    fn test_parse_peek_keyword() {
        // Test peek_keyword
        let parser = CypherParser::new("MATCH (n) RETURN n".to_string());
        assert!(parser.peek_keyword("MATCH"));

        let parser = CypherParser::new("  MATCH (n) RETURN n".to_string());
        assert!(parser.peek_keyword("MATCH"));

        let parser = CypherParser::new("MATCHING (n) RETURN n".to_string());
        assert!(!parser.peek_keyword("MATCH"));

        let parser = CypherParser::new("match (n) RETURN n".to_string());
        assert!(parser.peek_keyword("MATCH"));
    }

    #[test]
    fn test_parse_error_handling() {
        // Test error creation
        let parser = CypherParser::new("test".to_string());
        let error = parser.error("Test error");
        assert!(error.to_string().contains("Test error"));
        assert!(error.to_string().contains("line"));
        assert!(error.to_string().contains("column"));
    }

    #[test]
    fn test_parse_consume_char() {
        let mut parser = CypherParser::new("abc".to_string());

        assert_eq!(parser.consume_char(), Some('a'));
        assert_eq!(parser.pos, 1);
        assert_eq!(parser.line, 1);
        assert_eq!(parser.column, 2);

        assert_eq!(parser.consume_char(), Some('b'));
        assert_eq!(parser.pos, 2);
        assert_eq!(parser.line, 1);
        assert_eq!(parser.column, 3);

        assert_eq!(parser.consume_char(), Some('c'));
        assert_eq!(parser.pos, 3);
        assert_eq!(parser.line, 1);
        assert_eq!(parser.column, 4);

        assert_eq!(parser.consume_char(), None);
    }

    #[test]
    fn test_parse_consume_char_newline() {
        let mut parser = CypherParser::new("a\nb".to_string());

        assert_eq!(parser.consume_char(), Some('a'));
        assert_eq!(parser.line, 1);
        assert_eq!(parser.column, 2);

        assert_eq!(parser.consume_char(), Some('\n'));
        assert_eq!(parser.line, 2);
        assert_eq!(parser.column, 1);

        assert_eq!(parser.consume_char(), Some('b'));
        assert_eq!(parser.line, 2);
        assert_eq!(parser.column, 2);
    }

    #[test]
    fn test_parse_expect_char() {
        let mut parser = CypherParser::new("abc".to_string());

        assert!(parser.expect_char('a').is_ok());
        assert!(parser.expect_char('b').is_ok());
        assert!(parser.expect_char('c').is_ok());
        assert!(parser.expect_char('d').is_err());
    }

    #[test]
    fn test_parse_expect_keyword() {
        let mut parser = CypherParser::new("MATCH (n) RETURN n".to_string());

        assert!(parser.expect_keyword("MATCH").is_ok());
        assert!(parser.expect_keyword("WHERE").is_err());
    }

    #[test]
    fn test_parse_skip_whitespace() {
        let mut parser = CypherParser::new("   \t\n  abc".to_string());

        parser.skip_whitespace();
        assert_eq!(parser.pos, 7); // Should skip all whitespace (3 spaces + tab + newline + 2 spaces)
        assert_eq!(parser.peek_char(), Some('a'));
    }

    #[test]
    fn test_parse_peek_char() {
        let parser = CypherParser::new("abc".to_string());

        assert_eq!(parser.peek_char(), Some('a'));

        let parser = CypherParser::new("".to_string());
        assert_eq!(parser.peek_char(), None);
    }

    #[test]
    fn test_parse_peek_char_at() {
        let parser = CypherParser::new("abc".to_string());

        assert_eq!(parser.peek_char_at(0), Some('a'));
        assert_eq!(parser.peek_char_at(1), Some('b'));
        assert_eq!(parser.peek_char_at(2), Some('c'));
        assert_eq!(parser.peek_char_at(3), None);
    }

    #[test]
    fn test_parse_number() {
        let mut parser = CypherParser::new("123".to_string());
        let number = parser.parse_number().unwrap();
        assert_eq!(number, 123);

        let mut parser = CypherParser::new("abc".to_string());
        assert!(parser.parse_number().is_err());
    }

    #[test]
    fn test_parse_identifier() {
        let mut parser = CypherParser::new("abc123".to_string());
        let identifier = parser.parse_identifier().unwrap();
        assert_eq!(identifier, "abc123");

        let mut parser = CypherParser::new("_test".to_string());
        let identifier = parser.parse_identifier().unwrap();
        assert_eq!(identifier, "_test");

        let mut parser = CypherParser::new("123abc".to_string());
        assert!(parser.parse_identifier().is_err());
    }

    #[test]
    fn test_parse_keyword() {
        let mut parser = CypherParser::new("MATCH".to_string());
        let keyword = parser.parse_keyword().unwrap();
        assert_eq!(keyword, "MATCH");

        let mut parser = CypherParser::new("  MATCH  ".to_string());
        let keyword = parser.parse_keyword().unwrap();
        assert_eq!(keyword, "MATCH");
    }

    #[test]
    fn test_parse_with_clause() {
        let mut parser = CypherParser::new("WITH n, m.age AS age RETURN age".to_string());
        let query = parser.parse().unwrap();

        assert_eq!(query.clauses.len(), 2);

        match &query.clauses[0] {
            Clause::With(with_clause) => {
                assert_eq!(with_clause.items.len(), 2);
                assert!(!with_clause.distinct);
                assert!(with_clause.where_clause.is_none());

                match &with_clause.items[0].expression {
                    Expression::Variable(name) => assert_eq!(name, "n"),
                    _ => panic!("Expected variable expression"),
                }

                match &with_clause.items[1].expression {
                    Expression::PropertyAccess { variable, property } => {
                        assert_eq!(variable, "m");
                        assert_eq!(property, "age");
                    }
                    _ => panic!("Expected property access expression"),
                }

                assert_eq!(with_clause.items[1].alias, Some("age".to_string()));
            }
            _ => panic!("Expected WITH clause"),
        }
    }

    #[test]
    fn test_parse_with_distinct() {
        let mut parser = CypherParser::new("WITH DISTINCT n, m RETURN n".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[0] {
            Clause::With(with_clause) => {
                assert!(with_clause.distinct);
                assert_eq!(with_clause.items.len(), 2);
            }
            _ => panic!("Expected WITH clause"),
        }
    }

    #[test]
    fn test_parse_with_where() {
        let mut parser = CypherParser::new("WITH n WHERE n.age > 30 RETURN n".to_string());
        let query = parser.parse().unwrap();

        assert_eq!(query.clauses.len(), 2);

        match &query.clauses[0] {
            Clause::With(with_clause) => {
                assert!(with_clause.where_clause.is_some());
            }
            _ => panic!("Expected WITH clause"),
        }
    }

    #[test]
    fn test_with_clause_boundary() {
        let parser = CypherParser::new("WITH n".to_string());
        assert!(parser.is_clause_boundary());

        let parser = CypherParser::new("  WITH n".to_string());
        assert!(parser.is_clause_boundary());
    }

    #[test]
    fn test_parse_optional_match() {
        let mut parser = CypherParser::new(
            "OPTIONAL MATCH (p:Person)-[r:KNOWS]->(f:Person) RETURN p".to_string(),
        );
        let query = parser.parse().unwrap();

        assert!(!query.clauses.is_empty(), "Expected at least one clause");

        match &query.clauses[0] {
            Clause::Match(match_clause) => {
                assert!(match_clause.optional);
            }
            _ => panic!("Expected MATCH clause, got: {:?}", query.clauses[0]),
        }
    }

    #[test]
    fn test_parse_optional_match_with_where() {
        let mut parser = CypherParser::new(
            "MATCH (p:Person) OPTIONAL MATCH (p)-[r:KNOWS]->(f:Person) RETURN p, f".to_string(),
        );
        let query = parser.parse().unwrap();

        assert_eq!(query.clauses.len(), 3); // MATCH, OPTIONAL MATCH, RETURN

        match &query.clauses[0] {
            Clause::Match(match_clause) => {
                assert!(!match_clause.optional);
            }
            _ => panic!("Expected regular MATCH clause"),
        }

        match &query.clauses[1] {
            Clause::Match(match_clause) => {
                assert!(match_clause.optional);
            }
            _ => panic!("Expected OPTIONAL MATCH clause"),
        }
    }

    #[test]
    fn test_parse_multiple_optional_matches() {
        let mut parser = CypherParser::new(
            "MATCH (p:Person) OPTIONAL MATCH (p)-[r1]->(friend) OPTIONAL MATCH (p)-[r2]->(colleague) RETURN p, friend, colleague".to_string(),
        );
        let query = parser.parse().unwrap();

        assert_eq!(query.clauses.len(), 4); // MATCH, OPTIONAL MATCH, OPTIONAL MATCH, RETURN

        match &query.clauses[1] {
            Clause::Match(match_clause) => assert!(match_clause.optional),
            _ => panic!("Expected first OPTIONAL MATCH"),
        }

        match &query.clauses[2] {
            Clause::Match(match_clause) => assert!(match_clause.optional),
            _ => panic!("Expected second OPTIONAL MATCH"),
        }
    }

    #[test]
    fn test_parse_unwind_clause() {
        let mut parser = CypherParser::new("UNWIND [1, 2, 3] AS x RETURN x".to_string());
        let query = parser.parse().unwrap();

        assert!(!query.clauses.is_empty());

        match &query.clauses[0] {
            Clause::Unwind(unwind_clause) => {
                // Check that expression is parsed correctly
                assert!(matches!(&unwind_clause.expression, Expression::List(_)));
                assert_eq!(unwind_clause.variable, "x");
            }
            _ => panic!("Expected UNWIND clause"),
        }
    }

    #[test]
    fn test_unwind_clause_boundary() {
        let parser = CypherParser::new("UNWIND [1, 2, 3] AS x".to_string());
        assert!(parser.is_clause_boundary());

        let parser = CypherParser::new("  UNWIND [1, 2, 3] AS x".to_string());
        assert!(parser.is_clause_boundary());
    }

    #[test]
    fn test_parse_union_clause() {
        let mut parser = CypherParser::new(
            "MATCH (n:Person) RETURN n UNION MATCH (m:Company) RETURN m".to_string(),
        );
        let query = parser.parse().unwrap();

        // UNION splits into two separate queries
        assert_eq!(query.clauses.len(), 5); // MATCH, RETURN, UNION, MATCH, RETURN

        match &query.clauses[2] {
            Clause::Union(union_clause) => {
                assert_eq!(union_clause.union_type, UnionType::Distinct);
            }
            _ => panic!("Expected UNION clause"),
        }
    }

    #[test]
    fn test_parse_union_all_clause() {
        let mut parser = CypherParser::new(
            "MATCH (n:Person) RETURN n UNION ALL MATCH (m:Company) RETURN m".to_string(),
        );
        let query = parser.parse().unwrap();

        // UNION ALL splits into two separate queries
        assert_eq!(query.clauses.len(), 5); // MATCH, RETURN, UNION ALL, MATCH, RETURN

        // Check that UNION ALL clause is parsed
        let has_union = query.clauses.iter().any(|c| matches!(c, Clause::Union(_)));
        assert!(has_union, "Expected UNION ALL clause in query");

        // Find the UNION clause and check its type
        for clause in &query.clauses {
            if let Clause::Union(union_clause) = clause {
                assert_eq!(union_clause.union_type, UnionType::All);
                return;
            }
        }
        panic!("Expected UNION ALL clause");
    }

    #[test]
    fn test_union_clause_boundary() {
        let parser = CypherParser::new("UNION MATCH (n) RETURN n".to_string());
        assert!(parser.is_clause_boundary());

        let parser = CypherParser::new("  UNION ALL MATCH (n) RETURN n".to_string());
        assert!(parser.is_clause_boundary());
    }

    #[test]
    fn test_is_null_parsing() {
        let mut parser = CypherParser::new(
            "MATCH (n:Node) WHERE n.value IS NOT NULL RETURN count(*) AS count".to_string(),
        );
        let query = parser.parse().unwrap();

        assert_eq!(query.clauses.len(), 3); // MATCH, WHERE, RETURN

        // Check WHERE clause contains IsNull expression
        match &query.clauses[1] {
            Clause::Where(where_clause) => match &where_clause.expression {
                Expression::IsNull { expr, negated } => {
                    assert!(*negated, "Should be IS NOT NULL");
                    match &**expr {
                        Expression::PropertyAccess { variable, property } => {
                            assert_eq!(variable, "n");
                            assert_eq!(property, "value");
                        }
                        _ => panic!("Expected PropertyAccess in IsNull expression"),
                    }
                }
                _ => panic!(
                    "Expected IsNull expression in WHERE clause, got: {:?}",
                    where_clause.expression
                ),
            },
            _ => panic!("Expected WHERE clause"),
        }
    }

    #[test]
    fn test_is_null_simple() {
        let mut parser = CypherParser::new("MATCH (n) WHERE n.prop IS NULL RETURN n".to_string());
        let query = parser.parse().unwrap();

        match &query.clauses[1] {
            Clause::Where(where_clause) => match &where_clause.expression {
                Expression::IsNull { negated, .. } => {
                    assert!(!*negated, "Should be IS NULL");
                }
                _ => panic!("Expected IsNull expression"),
            },
            _ => panic!("Expected WHERE clause"),
        }
    }

    #[test]
    fn test_is_null_expression_only() {
        // Simulate what execute_filter does - parse just the expression
        let mut parser = CypherParser::new("n.value IS NOT NULL".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::IsNull {
                expr: inner,
                negated,
            } => {
                assert!(negated, "Should be IS NOT NULL");
                match *inner {
                    Expression::PropertyAccess { variable, property } => {
                        assert_eq!(variable, "n");
                        assert_eq!(property, "value");
                    }
                    _ => panic!("Expected PropertyAccess"),
                }
            }
            _ => panic!("Expected IsNull expression, got: {:?}", expr),
        }
    }

    #[test]
    fn test_is_null_expression_simple() {
        let mut parser = CypherParser::new("n.prop IS NULL".to_string());
        let expr = parser.parse_expression().unwrap();

        match expr {
            Expression::IsNull { negated, .. } => {
                assert!(!negated, "Should be IS NULL");
            }
            _ => panic!("Expected IsNull expression, got: {:?}", expr),
        }
    }

    #[test]
    fn test_and_with_comparisons() {
        let mut parser = CypherParser::new("n.age >= 25 AND n.age <= 35".to_string());
        let expr = parser.parse_expression().unwrap();

        // Should be: BinaryOp(>=) AND BinaryOp(<=)
        match expr {
            Expression::BinaryOp { left, op, right } => {
                assert!(matches!(op, BinaryOperator::And), "Top level should be AND");

                // Left side: n.age >= 25
                match &*left {
                    Expression::BinaryOp { op, .. } => {
                        assert!(
                            matches!(op, BinaryOperator::GreaterThanOrEqual),
                            "Left should be >="
                        );
                    }
                    _ => panic!("Left side should be BinaryOp, got: {:?}", left),
                }

                // Right side: n.age <= 35
                match &*right {
                    Expression::BinaryOp { op, .. } => {
                        assert!(
                            matches!(op, BinaryOperator::LessThanOrEqual),
                            "Right should be <="
                        );
                    }
                    _ => panic!("Right side should be BinaryOp, got: {:?}", right),
                }
            }
            _ => panic!("Expected BinaryOp with AND, got: {:?}", expr),
        }
    }
}
