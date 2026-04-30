//! Cypher AST node definitions — clauses, patterns, expressions, literals,
//! operators. No parsing logic lives here; the recursive-descent parser is
//! in `super::CypherParser`.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Abstract Syntax Tree for Cypher queries
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CypherQuery {
    /// Query clauses in order
    pub clauses: Vec<Clause>,
    /// Query parameters
    pub params: HashMap<String, serde_json::Value>,
    /// Optional leading `GRAPH[name]` scope
    /// (phase6_opencypher-advanced-types §6). `None` means the query
    /// runs against the session's current database. `Some(name)`
    /// overrides the scope for this one query; the session's database
    /// remains untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_scope: Option<String>,
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
    /// ALTER DATABASE command
    AlterDatabase(AlterDatabaseClause),
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
    /// SAVEPOINT <name> — create a new marker on the current
    /// transaction's savepoint stack (phase6_opencypher-advanced-types §5).
    Savepoint(SavepointClause),
    /// ROLLBACK TO SAVEPOINT <name> — undo everything since the named
    /// savepoint, keeping the savepoint and transaction active.
    RollbackToSavepoint(SavepointClause),
    /// RELEASE SAVEPOINT <name> — pop the named savepoint (and any
    /// inner savepoints) without undoing work.
    ReleaseSavepoint(SavepointClause),
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
    /// CALL procedure clause
    CallProcedure(CallProcedureClause),
    /// LOAD CSV clause for importing CSV data
    LoadCsv(LoadCsvClause),
    /// SHOW FUNCTIONS command
    ShowFunctions,
    /// SHOW CONSTRAINTS command
    ShowConstraints,
    /// SHOW QUERIES command
    ShowQueries,
    /// TERMINATE QUERY command
    TerminateQuery(TerminateQueryClause),
    /// CREATE FUNCTION command
    CreateFunction(CreateFunctionClause),
    /// DROP FUNCTION command
    DropFunction(DropFunctionClause),
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
    /// `SET lhs += mapExpr` — merge the map into the target's property
    /// bag. Keys present in `map` with non-NULL values overwrite the
    /// target's existing values; keys with NULL values are removed;
    /// keys absent from `map` are preserved. Distinct from
    /// `SET lhs = mapExpr` which replaces the entire bag.
    MapMerge {
        /// Target variable
        target: String,
        /// Map expression to merge
        map: Expression,
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

impl Pattern {
    /// Walk `elements` and replace every `QuantifiedGroup` that
    /// `QuantifiedGroup::try_lower_to_var_length_rel` can lower with
    /// the resulting `RelationshipPattern`, leaving the rest in place.
    ///
    /// This is the slice-1 fast path for QPP execution: any group of
    /// the shape `( ()-[:T]->() ){m,n}` collapses to the legacy
    /// `*m..n` form and rides the existing `VariableLengthPath`
    /// operator, no new operator required. Groups that carry inner
    /// state (named/labelled boundary nodes, multi-hop bodies, etc.)
    /// stay as `QuantifiedGroup` and the planner surfaces a clean
    /// `ERR_QPP_NOT_IMPLEMENTED` for them until the dedicated
    /// `QuantifiedExpand` operator lands.
    ///
    /// Returns a new pattern; the input is not mutated.
    #[must_use]
    pub fn lowered_for_planner(&self) -> Self {
        let elements = self
            .elements
            .iter()
            .map(|el| match el {
                PatternElement::QuantifiedGroup(group) => match group.try_lower_to_var_length_rel()
                {
                    Some(rel) => PatternElement::Relationship(rel),
                    None => el.clone(),
                },
                _ => el.clone(),
            })
            .collect();
        Self {
            elements,
            path_variable: self.path_variable.clone(),
        }
    }
}

/// Pattern element (node, relationship, or quantified group)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternElement {
    /// Node pattern
    Node(NodePattern),
    /// Relationship pattern
    Relationship(RelationshipPattern),
    /// Cypher 25 / GQL quantified path pattern: `( fragment ) quantifier`.
    /// Inner variables are list-promoted in the outer scope on match.
    QuantifiedGroup(QuantifiedGroup),
}

/// Quantified path pattern group (Cypher 25 / GQL).
///
/// The `inner` field is a nested sub-pattern that the planner
/// iteratively expands `quantifier` times. Unlike the relationship
/// quantifier, this one applies to whole path fragments — every
/// variable declared in `inner` becomes a `LIST<T>` in the outer
/// scope, ordered by iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantifiedGroup {
    /// Body of the group — must start and end with a node pattern
    /// (or be empty-legal when the quantifier allows zero matches).
    pub inner: Vec<PatternElement>,
    /// Quantifier applied to the whole group.
    pub quantifier: RelationshipQuantifier,
    /// Optional inner `WHERE` predicate evaluated against the
    /// per-iteration bindings before the iteration's row is
    /// emitted. The expression can reference any variable
    /// declared in `inner` (boundary-node vars and the inner
    /// relationship var) — at evaluation time those names see
    /// the values from the *current* iteration, not the
    /// list-promoted outer-scope `LIST<T>` form. An iteration
    /// that fails the predicate is dropped silently from the
    /// emitted row set; the BFS keeps walking past it.
    ///
    /// `None` for QPP groups without `WHERE`. Anonymous-body
    /// shapes that get lowered at parse time never carry a
    /// predicate (the lowering only fires when no inner state
    /// exists to filter on); the field is preserved across
    /// `try_lower_to_var_length_rel` only as a check that
    /// rejects the lowering when a predicate is present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub where_clause: Option<Expression>,
    /// Path-traversal mode parsed from the optional preceding
    /// `WALK | TRAIL | ACYCLIC | SIMPLE` keyword (or
    /// `REPEATABLE_ELEMENTS | DIFFERENT_RELATIONSHIPS |
    /// DIFFERENT_NODES_AND_RELATIONSHIPS` in the GQL aliases). When
    /// no keyword is written, defaults to
    /// [`crate::executor::types::QppMode::Walk`] — the historical
    /// engine behaviour.
    #[serde(default)]
    pub mode: crate::executor::types::QppMode,
    /// `true` when the parser saw an explicit mode keyword
    /// (`WALK | TRAIL | ACYCLIC | SIMPLE`) preceding the QPP
    /// group; `false` for the implicit-WALK default. Used by
    /// `try_lower_to_var_length_rel` to decide whether to keep
    /// the legacy `*m..n` fast path on (implicit only) or force
    /// the dedicated `QuantifiedExpand` operator (any explicit
    /// keyword, including `WALK`).
    #[serde(default)]
    pub mode_explicit: bool,
}

impl QuantifiedGroup {
    /// Try to lower this QPP group to a single quantified relationship
    /// pattern, equivalent to the legacy Cypher 9 `*m..n` form.
    ///
    /// This is the "fast path" for the common shape
    /// `( ()-[:T]->() ){m,n}` (anonymous boundary nodes, single
    /// relationship, no inner predicates). When all of these hold:
    ///
    /// - the body is exactly `Node, Relationship, Node`
    /// - the boundary nodes carry no variable, labels, or properties
    /// - the inner relationship itself is not already quantified
    ///
    /// the QPP collapses to `(outer_a)-[r:T*m..n]->(outer_b)` and
    /// can be planned by the existing variable-length expand path
    /// without needing the dedicated `QuantifiedExpand` operator.
    ///
    /// Anything else (named/labelled inner nodes, multi-hop bodies,
    /// inner property maps, intermediate filters that depend on
    /// list-promoted bindings) returns `None` and is left for the
    /// `QuantifiedExpand` operator coming in a follow-up slice of
    /// `phase6_opencypher-quantified-path-patterns`.
    pub fn try_lower_to_var_length_rel(&self) -> Option<RelationshipPattern> {
        // Inner WHERE predicates always force the dedicated
        // operator — the legacy `*m..n` operator has no slot for a
        // per-iteration predicate, and silently dropping the
        // predicate would produce wrong rows.
        if self.where_clause.is_some() {
            return None;
        }
        // Any explicit path-mode keyword (`WALK | TRAIL | ACYCLIC
        // | SIMPLE`) routes through the dedicated
        // `QuantifiedExpand` operator. The legacy variable-length-
        // path operator does not honour mode keywords (it has no
        // visited-set tracking), and even an explicit `WALK`
        // selects QPP-flavoured semantics that the legacy operator
        // does not match (the legacy operator collapses paths via
        // wavefront dedup, which the explicit-WALK contract on
        // QuantifiedExpand does not). The implicit (no-keyword)
        // default keeps the lowering on so the textbook anonymous-
        // body shape continues to take the legacy path byte-for-
        // byte unchanged.
        if self.mode_explicit {
            return None;
        }
        // Must be exactly Node, Relationship, Node.
        if self.inner.len() != 3 {
            return None;
        }
        let (start, rel, end) = match (&self.inner[0], &self.inner[1], &self.inner[2]) {
            (
                PatternElement::Node(start),
                PatternElement::Relationship(rel),
                PatternElement::Node(end),
            ) => (start, rel, end),
            _ => return None,
        };

        // Boundary nodes inside the QPP must be pure glue — they exist
        // only to anchor the relationship inside the parenthesised
        // body. Anything carrying user state (variable, labels,
        // properties) participates in list promotion and forces the
        // full operator.
        let is_glue = |np: &NodePattern| {
            np.variable.is_none() && np.labels.is_empty() && np.properties.is_none()
        };
        if !is_glue(start) || !is_glue(end) {
            return None;
        }

        // The inner relationship itself must not be quantified — we
        // are about to fuse the QPP quantifier in, and stacking two
        // quantifiers (`( ()-[:T*1..3]->() ){1,5}`) is not the
        // single-rel shorthand this lowering targets.
        if rel.quantifier.is_some() {
            return None;
        }

        Some(RelationshipPattern {
            variable: rel.variable.clone(),
            types: rel.types.clone(),
            direction: rel.direction,
            properties: rel.properties.clone(),
            quantifier: Some(self.quantifier.clone()),
        })
    }
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

/// Savepoint clause payload.
///
/// Carried by [`Clause::Savepoint`], [`Clause::RollbackToSavepoint`] and
/// [`Clause::ReleaseSavepoint`]. The `name` is the user-supplied
/// identifier; the runtime enforces uniqueness inside a transaction on
/// push (nested savepoints with the same name shadow each other and
/// unwind LIFO).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavepointClause {
    /// Savepoint name (ASCII identifier).
    pub name: String,
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

/// ALTER DATABASE clause
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlterDatabaseClause {
    /// Database name
    pub name: String,
    /// Alteration type
    pub alteration: DatabaseAlteration,
}

/// Database alteration types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DatabaseAlteration {
    /// Set access mode (READ WRITE or READ ONLY)
    SetAccess { read_only: bool },
    /// Set option (generic key-value)
    SetOption { key: String, value: String },
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
    /// Optional index name (`CREATE INDEX <name> FOR ...`).
    ///
    /// `None` for the legacy `CREATE INDEX ON :Label(prop)` shape,
    /// `Some(name)` for the Cypher 25 `CREATE INDEX <name> FOR (n:Label) ON (...)`
    /// form introduced in phase6_opencypher-advanced-types §3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Label name
    pub label: String,
    /// Property name (first element of `properties`, retained for
    /// backwards compatibility with existing single-property callers).
    pub property: String,
    /// All property names the index keys on. Length 1 for a regular
    /// single-property index, length ≥ 2 for a composite B-tree index.
    /// phase6_opencypher-advanced-types §3.
    #[serde(default)]
    pub properties: Vec<String>,
    /// Optional IF NOT EXISTS flag
    pub if_not_exists: bool,
    /// Optional OR REPLACE flag
    pub or_replace: bool,
    /// Index type (None = property index, Some("spatial") = spatial index)
    pub index_type: Option<String>,
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
    /// Optional constraint name (`CREATE CONSTRAINT <name> FOR ...`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Constraint type
    pub constraint_type: ConstraintType,
    /// Label name (node constraints) or relationship type (rel constraints).
    pub label: String,
    /// Primary property name (first element of `properties`, retained
    /// for backward compatibility with single-property callers).
    pub property: String,
    /// Every property the constraint keys on. Length 1 for UNIQUE /
    /// EXISTS / IS NOT NULL / property-type, length ≥ 2 for NODE KEY.
    /// phase6_opencypher-constraint-enforcement §5/§7.
    #[serde(default)]
    pub properties: Vec<String>,
    /// Entity scope — NODE or RELATIONSHIP. Defaults to NODE so
    /// legacy callers building `CreateConstraintClause` literals keep
    /// compiling.
    #[serde(default)]
    pub entity: ConstraintEntity,
    /// Property-type token when `constraint_type == PropertyType`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property_type: Option<String>,
    /// Optional IF NOT EXISTS flag
    pub if_not_exists: bool,
}

/// Entity scope for a constraint.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintEntity {
    #[default]
    Node,
    Relationship,
}

/// Constraint type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintType {
    /// UNIQUE constraint (node property uniqueness).
    Unique,
    /// EXISTS constraint — property must exist (alias for
    /// node/rel NOT NULL).
    Exists,
    /// NODE KEY — `(p1, p2, ...)` composite uniqueness + per-component
    /// NOT NULL (phase6_opencypher-constraint-enforcement §5).
    NodeKey,
    /// Property-type constraint — `IS :: <TYPE>` (§7).
    PropertyType,
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

/// CREATE FUNCTION clause
/// Syntax: CREATE FUNCTION name(param1: Type1, param2: Type2) [IF NOT EXISTS] AS expression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFunctionClause {
    /// Function name
    pub name: String,
    /// Function parameters
    pub parameters: Vec<UdfParameter>,
    /// Return type
    pub return_type: crate::udf::UdfReturnType,
    /// Optional IF NOT EXISTS flag
    pub if_not_exists: bool,
    /// Function description (optional)
    pub description: Option<String>,
}

/// DROP FUNCTION clause
/// Syntax: DROP FUNCTION name [IF EXISTS]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DropFunctionClause {
    /// Function name
    pub name: String,
    /// Optional IF EXISTS flag
    pub if_exists: bool,
}

/// TERMINATE QUERY clause
/// Syntax: TERMINATE QUERY 'query-id'
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminateQueryClause {
    /// Query ID to terminate
    pub query_id: String,
}

/// UDF parameter (re-exported from udf module for parser)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdfParameter {
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: crate::udf::UdfReturnType,
    /// Whether parameter is required
    pub required: bool,
    /// Default value (if optional)
    pub default: Option<serde_json::Value>,
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
    /// phase6_opencypher-subquery-transactions — `IN CONCURRENT
    /// TRANSACTIONS` variant. When `Some(n)`, spawn up to `n`
    /// parallel workers; when `None`, single-worker (default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concurrency: Option<usize>,
    /// phase6_opencypher-subquery-transactions — `ON ERROR`
    /// clause. Default is `Fail` (abort immediately), matching
    /// Neo4j's implicit behaviour.
    #[serde(default, skip_serializing_if = "OnErrorPolicy::is_default")]
    pub on_error: OnErrorPolicy,
    /// phase6_opencypher-subquery-transactions — `REPORT STATUS
    /// AS <var>`. When set, the clause emits one row per batch
    /// with columns `(started, committed, rowsProcessed, err)`
    /// bound to the named variable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_var: Option<String>,
    /// phase6_opencypher-subquery-transactions §8 — Cypher 25
    /// `CALL (var1, var2, …) { … }` import-list form. When `Some`,
    /// only the listed outer variables are visible inside the inner
    /// scope; every other outer binding is shadowed. When `None`,
    /// the legacy "everything visible" rule applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub import_list: Option<Vec<String>>,
}

/// phase6_opencypher-subquery-transactions — `ON ERROR` clause
/// variants. Controls how `CALL { ... } IN TRANSACTIONS` reacts to
/// a failure inside the inner subquery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum OnErrorPolicy {
    /// `ON ERROR FAIL` (default) — abort the outer query on any
    /// inner failure.
    #[default]
    Fail,
    /// `ON ERROR CONTINUE` — log + mark row failed, keep going.
    Continue,
    /// `ON ERROR BREAK` — commit the current batch, stop cleanly.
    Break,
    /// `ON ERROR RETRY <n>` — retry the failing batch up to `n`
    /// times before giving up.
    Retry { max_attempts: usize },
}

impl OnErrorPolicy {
    /// Serde helper — skip the field when it carries the default.
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Fail)
    }
}

/// CALL procedure clause
/// Syntax: CALL procedure.name(arg1, arg2, ...) [YIELD column1, column2, ...]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallProcedureClause {
    /// Procedure name (e.g., "gds.shortestPath.dijkstra")
    pub procedure_name: String,
    /// Procedure arguments (as expressions)
    pub arguments: Vec<Expression>,
    /// YIELD clause (optional) - columns to return
    pub yield_columns: Option<Vec<String>>,
}

/// LOAD CSV clause for importing CSV data
/// Syntax: LOAD CSV FROM 'file:///path/to/file.csv' [WITH HEADERS] [FIELDTERMINATOR ','] AS row
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadCsvClause {
    /// CSV file URL/path
    pub url: String,
    /// Variable name to bind each row to (default: 'row')
    pub variable: String,
    /// Whether CSV has headers (WITH HEADERS)
    pub with_headers: bool,
    /// Field terminator character (default: ',')
    pub field_terminator: Option<String>,
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
    /// Array index access (expression[index])
    ArrayIndex {
        /// Base expression (array or property)
        base: Box<Expression>,
        /// Index expression
        index: Box<Expression>,
    },
    /// Array slice access (expression[start..end])
    ArraySlice {
        /// Base expression (array or property)
        base: Box<Expression>,
        /// Start index (inclusive, optional)
        start: Option<Box<Expression>>,
        /// End index (exclusive, optional)
        end: Option<Box<Expression>>,
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
    /// `COLLECT { … }` subquery expression
    /// (phase6_opencypher-subquery-transactions §9 / Cypher 25).
    ///
    /// Runs the inner query against the current outer-row scope and
    /// folds every emitted row into a LIST value:
    ///
    /// - single-column inner → `LIST<T>` of that column's values,
    /// - multi-column inner  → `LIST<MAP>` keyed by the column names,
    /// - aggregating inner   → single-element list (the aggregation
    ///   produces exactly one row).
    ///
    /// `Box<CypherQuery>` keeps the AST node sized; the inner query
    /// re-uses the regular clause vocabulary (MATCH / WHERE / WITH /
    /// RETURN), so the existing planner + evaluator stack lights it
    /// up without bespoke recursion machinery.
    CollectSubquery {
        /// Inner query AST; must contain at least one clause and
        /// terminate with a RETURN clause (validated at parse time).
        inner: Box<CypherQuery>,
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
    /// Point literal (geospatial)
    Point(crate::geospatial::Point),
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

// Hash on the Clause discriminant only — enough to key query-plan caches
// without needing to hash every nested expression.
impl std::hash::Hash for Clause {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
    }
}
