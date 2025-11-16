//! Query builder for constructing Cypher queries in a type-safe manner

use crate::models::Value;
use std::collections::HashMap;

/// Query builder for constructing Cypher queries
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    parts: Vec<String>,
    params: HashMap<String, Value>,
    current_clause: Option<ClauseType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClauseType {
    Match,
    Create,
    Merge,
    Where,
    Return,
    Set,
    Delete,
    With,
    OrderBy,
    Limit,
    Skip,
}

impl QueryBuilder {
    /// Create a new query builder
    pub fn new() -> Self {
        Self {
            parts: Vec::new(),
            params: HashMap::new(),
            current_clause: None,
        }
    }

    /// Add a MATCH clause
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nexus_sdk::query_builder::QueryBuilder;
    ///
    /// let query = QueryBuilder::new()
    ///     .match_("(n:Person)")
    ///     .build();
    /// ```
    pub fn match_(mut self, pattern: &str) -> Self {
        self.parts.push(format!("MATCH {}", pattern));
        self.current_clause = Some(ClauseType::Match);
        self
    }

    /// Add a CREATE clause
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nexus_sdk::query_builder::QueryBuilder;
    ///
    /// let query = QueryBuilder::new()
    ///     .create("(n:Person {name: $name})")
    ///     .param("name", "Alice")
    ///     .build();
    /// ```
    pub fn create(mut self, pattern: &str) -> Self {
        self.parts.push(format!("CREATE {}", pattern));
        self.current_clause = Some(ClauseType::Create);
        self
    }

    /// Add a MERGE clause
    pub fn merge(mut self, pattern: &str) -> Self {
        self.parts.push(format!("MERGE {}", pattern));
        self.current_clause = Some(ClauseType::Merge);
        self
    }

    /// Add a WHERE clause
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nexus_sdk::query_builder::QueryBuilder;
    ///
    /// let query = QueryBuilder::new()
    ///     .match_("(n:Person)")
    ///     .where_("n.age > $min_age")
    ///     .param("min_age", 18)
    ///     .build();
    /// ```
    pub fn where_(mut self, condition: &str) -> Self {
        self.parts.push(format!("WHERE {}", condition));
        self.current_clause = Some(ClauseType::Where);
        self
    }

    /// Add a RETURN clause
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nexus_sdk::query_builder::QueryBuilder;
    ///
    /// let query = QueryBuilder::new()
    ///     .match_("(n:Person)")
    ///     .return_("n")
    ///     .build();
    /// ```
    pub fn return_(mut self, items: &str) -> Self {
        self.parts.push(format!("RETURN {}", items));
        self.current_clause = Some(ClauseType::Return);
        self
    }

    /// Add a SET clause
    pub fn set(mut self, assignments: &str) -> Self {
        self.parts.push(format!("SET {}", assignments));
        self.current_clause = Some(ClauseType::Set);
        self
    }

    /// Add a DELETE clause
    pub fn delete(mut self, items: &str) -> Self {
        self.parts.push(format!("DELETE {}", items));
        self.current_clause = Some(ClauseType::Delete);
        self
    }

    /// Add a WITH clause
    pub fn with(mut self, items: &str) -> Self {
        self.parts.push(format!("WITH {}", items));
        self.current_clause = Some(ClauseType::With);
        self
    }

    /// Add an ORDER BY clause
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nexus_sdk::query_builder::QueryBuilder;
    ///
    /// let query = QueryBuilder::new()
    ///     .match_("(n:Person)")
    ///     .return_("n")
    ///     .order_by("n.name ASC")
    ///     .build();
    /// ```
    pub fn order_by(mut self, expression: &str) -> Self {
        self.parts.push(format!("ORDER BY {}", expression));
        self.current_clause = Some(ClauseType::OrderBy);
        self
    }

    /// Add a LIMIT clause
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nexus_sdk::query_builder::QueryBuilder;
    ///
    /// let query = QueryBuilder::new()
    ///     .match_("(n:Person)")
    ///     .return_("n")
    ///     .limit(10)
    ///     .build();
    /// ```
    pub fn limit(mut self, count: usize) -> Self {
        self.parts.push(format!("LIMIT {}", count));
        self.current_clause = Some(ClauseType::Limit);
        self
    }

    /// Add a SKIP clause
    pub fn skip(mut self, count: usize) -> Self {
        self.parts.push(format!("SKIP {}", count));
        self.current_clause = Some(ClauseType::Skip);
        self
    }

    /// Add a parameter to the query
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nexus_sdk::query_builder::QueryBuilder;
    ///
    /// let query = QueryBuilder::new()
    ///     .match_("(n:Person)")
    ///     .where_("n.name = $name")
    ///     .param("name", "Alice")
    ///     .build();
    /// ```
    pub fn param<T: Into<Value>>(mut self, name: &str, value: T) -> Self {
        self.params.insert(name.to_string(), value.into());
        self
    }

    /// Build the final query string
    pub fn build(self) -> BuiltQuery {
        BuiltQuery {
            query: self.parts.join(" "),
            params: if self.params.is_empty() {
                None
            } else {
                Some(self.params)
            },
        }
    }
}

impl Default for QueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Built query ready for execution
#[derive(Debug, Clone)]
pub struct BuiltQuery {
    query: String,
    params: Option<HashMap<String, Value>>,
}

impl BuiltQuery {
    /// Get the query string
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Get the parameters
    pub fn params(&self) -> Option<&HashMap<String, Value>> {
        self.params.as_ref()
    }

    /// Convert to query string and parameters tuple
    pub fn into_parts(self) -> (String, Option<HashMap<String, Value>>) {
        (self.query, self.params)
    }
}
