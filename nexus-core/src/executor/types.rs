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
            enable_advanced_joins: true,
            enable_relationship_optimizations: true,
            enable_numa_optimizations: false, // Disabled by default (requires NUMA hardware)
            enable_numa_caching: false,       // Disabled by default (requires NUMA hardware)
            enable_lock_free_structures: true, // Enabled by default (always beneficial)
        }
    }
}

/// Cypher query
#[derive(Debug, Clone)]
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
