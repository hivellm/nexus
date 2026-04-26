//! Public types for the Cypher executor: query/result records, physical
//! operator variants, aggregations, join/index kinds, and the executor
//! configuration struct. No execution logic lives here.

use super::parser;
use serde_json::Value;
use std::collections::HashMap;

/// Executor configuration for controlling execution behavior
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Enable vectorized execution for better performance on large datasets
    pub enable_vectorized_execution: bool,
    /// Enable JIT compilation for frequently executed queries
    pub enable_jit_compilation: bool,
    /// Enable parallel execution for CPU-intensive operations
    pub enable_parallel_execution: bool,
    /// Minimum dataset size to trigger vectorized operations
    pub vectorized_threshold: usize,
    /// Minimum row count at which the filter / groupless-aggregate
    /// operators materialise a columnar batch and dispatch through
    /// the SIMD kernels in `crate::simd::compare` + `crate::simd::reduce`.
    ///
    /// Below this threshold the row-at-a-time path stays active — the
    /// columnar materialisation has a non-zero per-batch cost that
    /// only amortises on large inputs. Tuned on the reference
    /// benchmark hardware; see `phase3_executor-columnar-wiring` for
    /// the tuning rationale.
    pub columnar_threshold: usize,
    /// Enable advanced join algorithms (hash joins, merge joins)
    pub enable_advanced_joins: bool,
    /// Enable relationship processing optimizations (specialized storage, advanced traversal, property indexing)
    pub enable_relationship_optimizations: bool,
    /// Phase 9: Enable NUMA-aware memory allocation and thread scheduling
    pub enable_numa_optimizations: bool,
    /// Phase 9: Enable advanced caching strategies with NUMA partitioning
    pub enable_numa_caching: bool,
    /// Phase 9: Enable lock-free data structures where possible
    pub enable_lock_free_structures: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            enable_vectorized_execution: true,
            enable_jit_compilation: true,
            // Parallel execution stays off by default until stability testing
            // completes; flip via ExecutorConfig when opting in.
            enable_parallel_execution: false,
            vectorized_threshold: 50,
            // 4096 rows matches the proposal's tuning target — big
            // enough that msgpack + page-cache overhead dominates,
            // small enough that an in-flight query over a
            // medium-sized label slice still benefits.
            columnar_threshold: 4096,
            enable_advanced_joins: true,
            enable_relationship_optimizations: true,
            enable_numa_optimizations: false, // Disabled by default (requires NUMA hardware)
            enable_numa_caching: false,       // Disabled by default (requires NUMA hardware)
            enable_lock_free_structures: true, // Enabled by default (always beneficial)
        }
    }
}

/// Cypher query
#[derive(Debug, Clone, Default)]
pub struct Query {
    /// Query string
    pub cypher: String,
    /// Query parameters
    pub params: HashMap<String, Value>,
}

/// Query result row
#[derive(Debug, Clone)]
pub struct Row {
    /// Column values
    pub values: Vec<serde_json::Value>,
}

/// Query result set
#[derive(Debug, Clone, Default)]
pub struct ResultSet {
    /// Column names
    pub columns: Vec<String>,
    /// Result rows
    pub rows: Vec<Row>,
}

/// Execution plan containing a sequence of operators
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Sequence of operators to execute
    pub operators: Vec<Operator>,
}

/// Physical operator
#[derive(Debug, Clone)]
pub enum Operator {
    /// Scan nodes by label
    NodeByLabel {
        /// Label ID
        label_id: u32,
        /// Variable name
        variable: String,
    },
    /// Scan all nodes (no label filter)
    AllNodesScan {
        /// Variable name
        variable: String,
    },
    /// Filter by property predicate
    Filter {
        /// Predicate expression
        predicate: String,
    },
    /// Optional filter - preserves rows with NULL optional variables
    /// Used for WHERE clauses after OPTIONAL MATCH
    /// If predicate fails but optional_vars are involved, sets them to NULL instead of removing row
    OptionalFilter {
        /// Predicate expression
        predicate: String,
        /// Variables from OPTIONAL MATCH that should be set to NULL if predicate fails
        optional_vars: Vec<String>,
    },
    /// Expand relationships
    Expand {
        /// Type IDs (empty = all types, multiple types are OR'd together)
        type_ids: Vec<u32>,
        /// Direction (Outgoing, Incoming, Both)
        direction: Direction,
        /// Source variable
        source_var: String,
        /// Target variable
        target_var: String,
        /// Relationship variable
        rel_var: String,
        /// Optional (LEFT OUTER JOIN semantics - preserve rows with NULL if no match)
        optional: bool,
    },
    /// Project columns
    Project {
        /// Projection expressions with aliases
        items: Vec<ProjectionItem>,
    },
    /// WITH clause - project intermediate results and update context variables
    /// Unlike Project, this updates context.variables for subsequent clauses
    With {
        /// Projection expressions with aliases
        items: Vec<ProjectionItem>,
        /// DISTINCT flag
        distinct: bool,
    },
    /// Limit results
    Limit {
        /// Maximum rows
        count: usize,
    },
    /// Sort results by columns
    Sort {
        /// Columns to sort by
        columns: Vec<String>,
        /// Sort order (true = ascending, false = descending)
        ascending: Vec<bool>,
    },
    /// Aggregate results
    Aggregate {
        /// Group by columns
        group_by: Vec<String>,
        /// Aggregation functions
        aggregations: Vec<Aggregation>,
        /// Projection items (for evaluating literals in aggregation functions without MATCH)
        projection_items: Option<Vec<ProjectionItem>>,
        /// Source operator (for optimization analysis)
        source: Option<Box<Operator>>,
        /// Whether streaming optimization is applied
        streaming_optimized: bool,
        /// Whether push-down optimization is applied
        push_down_optimized: bool,
    },
    /// Union two result sets
    Union {
        /// Left operator pipeline
        left: Vec<Operator>,
        /// Right operator pipeline
        right: Vec<Operator>,
        /// Distinct flag (true = UNION, false = UNION ALL)
        distinct: bool,
    },
    /// Join two result sets
    Join {
        /// Left operand
        left: Box<Operator>,
        /// Right operand
        right: Box<Operator>,
        /// Join type
        join_type: JoinType,
        /// Join condition
        condition: Option<String>,
    },
    /// Create nodes and relationships from pattern
    Create {
        /// Pattern to create
        pattern: parser::Pattern,
    },
    /// Delete nodes (without detaching relationships)
    Delete {
        /// Variables to delete
        variables: Vec<String>,
    },
    /// Delete nodes and their relationships
    DetachDelete {
        /// Variables to delete
        variables: Vec<String>,
    },
    /// Scan using index
    IndexScan {
        /// Index name
        index_name: String,
        /// Label to scan
        label: String,
    },
    /// phase6_opencypher-advanced-types §3.4 — composite B-tree seek.
    ///
    /// Emitted by the planner when a query predicates a strict prefix
    /// of a registered composite index and the engine has a matching
    /// registry entry. The executor reads the registry, performs an
    /// exact (if every column is bound) or prefix seek, and emits one
    /// row per returned node id bound to `variable`.
    CompositeBtreeSeek {
        /// Label every indexed node carries.
        label: String,
        /// Variable the pattern assigns the returned nodes to.
        variable: String,
        /// Ordered property-value pairs that formed the prefix.
        /// Length ≥ 1, ≤ the index's arity. Equality only — a range
        /// predicate on the trailing column is expressed by the
        /// planner via a residual `Filter` on top of this operator.
        prefix: Vec<(String, serde_json::Value)>,
    },
    /// Distinct results
    Distinct {
        /// Columns to check for distinctness
        columns: Vec<String>,
    },
    /// Hash join operation
    HashJoin {
        /// Left join key
        left_key: String,
        /// Right join key
        right_key: String,
    },
    /// Unwind a list into rows
    Unwind {
        /// Expression that evaluates to a list
        expression: String,
        /// Variable name to bind each list item
        variable: String,
    },
    /// Variable-length path expansion
    VariableLengthPath {
        /// Type ID (None = all types)
        type_id: Option<u32>,
        /// Direction (Outgoing, Incoming, Both)
        direction: Direction,
        /// Source variable
        source_var: String,
        /// Target variable
        target_var: String,
        /// Relationship variable (optional, for collecting path relationships)
        rel_var: String,
        /// Path variable (optional, for collecting the full path)
        path_var: String,
        /// Quantifier specifying path length constraints
        quantifier: parser::RelationshipQuantifier,
    },
    /// Cypher 25 / GQL Quantified Path Pattern with an arbitrary-arity
    /// body. Drives BFS over the parenthesised fragment between
    /// `source_var` and `target_var`, emitting one row per accepted
    /// iteration count `k` in `[min_length, max_length]`. Each
    /// iteration walks every hop in `hops` left-to-right, in order,
    /// applying the per-hop relationship filter and the per-position
    /// node filter from `inner_nodes`. Inner boundary nodes and
    /// inner relationships are list-promoted to `LIST<NODE>` /
    /// `LIST<RELATIONSHIP>` per the GQL type rules — each list
    /// holds one entry per **iteration** (not per hop), keeping the
    /// `x[k]` indexing semantics regardless of body arity.
    ///
    /// Slice 1 lowers single-relationship anonymous-body shapes to
    /// the legacy `VariableLengthPath`; this operator picks up
    /// everything else (named/labelled inner nodes, multi-hop
    /// bodies, relationship-property filters, …).
    QuantifiedExpand {
        /// Outer variable holding the source node before the QPP body.
        source_var: String,
        /// Outer variable holding the target node after the QPP body.
        target_var: String,
        /// Sequence of inner relationship hops. `hops.len()` is the
        /// number of relationships per **iteration**; one full
        /// iteration walks every hop in order. Slice-2 single-rel
        /// bodies have `hops.len() == 1`.
        hops: Vec<QppHopSpec>,
        /// Inner boundary node specifications. Invariant:
        /// `inner_nodes.len() == hops.len() + 1`. `inner_nodes[i]`
        /// constrains the node *between* `hops[i-1]` and `hops[i]`
        /// (for `i == 0` it is the start of every iteration; for
        /// `i == hops.len()` it is the end).
        inner_nodes: Vec<QppNodeSpec>,
        /// Optional `WHERE` predicate written inside the body
        /// parentheses. Evaluated against the iteration's snapshot
        /// (boundary-node vars + relationship var bound to the
        /// values reached on that hop, *not* their list-promoted
        /// outer-scope form). An iteration that fails the
        /// predicate is dropped before emission.
        inner_where: Option<parser::Expression>,
        /// Lower bound on iteration count (inclusive).
        min_length: usize,
        /// Upper bound on iteration count (inclusive). `usize::MAX`
        /// for unbounded — the operator caps internally at
        /// `MAX_QPP_DEPTH` to keep BFS frames tractable.
        max_length: usize,
        /// Optional flag carried through from the surrounding
        /// `OPTIONAL MATCH`; preserves NULLs when no body matches.
        optional: bool,
    },
    /// Call a procedure
    CallProcedure {
        /// Procedure name (e.g., "gds.shortestPath.dijkstra")
        procedure_name: String,
        /// Procedure arguments (as expressions)
        arguments: Vec<parser::Expression>,
        /// YIELD columns (optional) - columns to return from procedure
        yield_columns: Option<Vec<String>>,
    },
    /// Load CSV file
    LoadCsv {
        /// CSV file URL/path
        url: String,
        /// Variable name to bind each row to
        variable: String,
        /// Whether CSV has headers
        with_headers: bool,
        /// Field terminator character (default: ',')
        field_terminator: Option<String>,
    },
    /// Create an index
    CreateIndex {
        /// Label name
        label: String,
        /// Property name
        property: String,
        /// Index type (None = property index, Some("spatial") = spatial index)
        index_type: Option<String>,
        /// IF NOT EXISTS flag
        if_not_exists: bool,
        /// OR REPLACE flag
        or_replace: bool,
    },
    /// Show all databases
    ShowDatabases,
    /// Create a new database
    CreateDatabase {
        /// Database name
        name: String,
        /// IF NOT EXISTS flag
        if_not_exists: bool,
    },
    /// Drop a database
    DropDatabase {
        /// Database name
        name: String,
        /// IF EXISTS flag
        if_exists: bool,
    },
    /// Alter a database
    AlterDatabase {
        /// Database name
        name: String,
        /// Access mode: true = read-only, false = read-write
        read_only: Option<bool>,
        /// Option key-value pair
        option: Option<(String, String)>,
    },
    /// Switch to a different database
    UseDatabase {
        /// Database name to switch to
        name: String,
    },
}

/// Projection entry describing an expression and its alias
#[derive(Debug, Clone)]
pub struct ProjectionItem {
    /// Expression to evaluate
    pub expression: parser::Expression,
    /// Alias to use in the result set
    pub alias: String,
}

/// One relationship hop inside a Quantified Path Pattern body.
///
/// A QPP body of arity `n` carries `n` of these spliced together
/// with `n + 1` `QppNodeSpec` entries describing the boundary
/// nodes between them. See `Operator::QuantifiedExpand` for the
/// invariants on the surrounding `hops` / `inner_nodes` slices.
#[derive(Debug, Clone)]
pub struct QppHopSpec {
    /// Allowed relationship type IDs (empty = all types).
    pub type_ids: Vec<u32>,
    /// Direction of this hop (independent of other hops in the body).
    pub direction: Direction,
    /// Optional relationship variable. List-promoted to
    /// `LIST<RELATIONSHIP>` on emission, ordered by iteration.
    pub var: Option<String>,
    /// Optional property-equality filter applied to this hop's
    /// relationship at runtime.
    pub properties: Option<parser::PropertyMap>,
}

/// One inner boundary-node specification inside a Quantified Path
/// Pattern body. The slice-2/3 operator applies the label and
/// property filters per accepted iteration; declared variables are
/// list-promoted to `LIST<NODE>` in the outer scope.
#[derive(Debug, Clone)]
pub struct QppNodeSpec {
    /// Optional inner-node variable. List-promoted to `LIST<NODE>`
    /// on emission.
    pub var: Option<String>,
    /// Label AND-filter applied per accepted iteration.
    pub labels: Vec<String>,
    /// Property-equality filter applied per accepted iteration.
    pub properties: Option<parser::PropertyMap>,
}

/// Relationship direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Outgoing edges
    Outgoing,
    /// Incoming edges
    Incoming,
    /// Both directions
    Both,
}

/// Aggregation function
#[derive(Debug, Clone)]
pub enum Aggregation {
    /// Count rows
    Count {
        /// Column to count (None = count all)
        column: Option<String>,
        /// Alias for result
        alias: String,
        /// Distinct flag for COUNT(DISTINCT ...)
        distinct: bool,
    },
    /// Sum values
    Sum {
        /// Column to sum
        column: String,
        /// Alias for result
        alias: String,
    },
    /// Average values
    Avg {
        /// Column to average
        column: String,
        /// Alias for result
        alias: String,
    },
    /// Minimum value
    Min {
        /// Column to find minimum
        column: String,
        /// Alias for result
        alias: String,
    },
    /// Maximum value
    Max {
        /// Column to find maximum
        column: String,
        /// Alias for result
        alias: String,
    },
    /// Collect values into array
    Collect {
        /// Column to collect
        column: String,
        /// Alias for result
        alias: String,
        /// Distinct flag for COLLECT(DISTINCT ...)
        distinct: bool,
    },
    /// Discrete percentile (nearest value)
    PercentileDisc {
        /// Column to calculate percentile
        column: String,
        /// Alias for result
        alias: String,
        /// Percentile value (0.0 to 1.0)
        percentile: f64,
    },
    /// Continuous percentile (interpolated)
    PercentileCont {
        /// Column to calculate percentile
        column: String,
        /// Alias for result
        alias: String,
        /// Percentile value (0.0 to 1.0)
        percentile: f64,
    },
    /// Sample standard deviation
    StDev {
        /// Column to calculate standard deviation
        column: String,
        /// Alias for result
        alias: String,
    },
    /// Population standard deviation
    StDevP {
        /// Column to calculate population standard deviation
        column: String,
        /// Alias for result
        alias: String,
    },
    /// Optimized COUNT(*) using index statistics
    CountStarOptimized {
        /// Alias for result
        alias: String,
    },
}

/// Join type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    /// Inner join
    Inner,
    /// Left outer join
    LeftOuter,
    /// Right outer join
    RightOuter,
    /// Full outer join
    FullOuter,
}

/// Index type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexType {
    /// Label index
    Label,
    /// Property index
    Property,
    /// KNN vector index
    Vector,
    /// Full-text index
    FullText,
    /// Spatial index (R-tree)
    Spatial,
}
