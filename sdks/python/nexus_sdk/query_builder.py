"""Query builder for constructing Cypher queries in a type-safe manner."""

from typing import Any, Dict, List, Optional, Tuple


class QueryBuilder:
    """Query builder for constructing Cypher queries."""

    def __init__(self):
        """Create a new query builder."""
        self._parts: List[str] = []
        self._params: Dict[str, Any] = {}
        self._current_clause: Optional[str] = None

    def match_(self, pattern: str) -> "QueryBuilder":
        """Add a MATCH clause.

        Args:
            pattern: Match pattern (e.g., "(n:Person)")

        Returns:
            QueryBuilder instance for chaining
        """
        self._parts.append(f"MATCH {pattern}")
        self._current_clause = "MATCH"
        return self

    def create(self, pattern: str) -> "QueryBuilder":
        """Add a CREATE clause.

        Args:
            pattern: Create pattern (e.g., "(n:Person {name: $name})")

        Returns:
            QueryBuilder instance for chaining
        """
        self._parts.append(f"CREATE {pattern}")
        self._current_clause = "CREATE"
        return self

    def merge(self, pattern: str) -> "QueryBuilder":
        """Add a MERGE clause.

        Args:
            pattern: Merge pattern (e.g., "(n:Person {name: $name})")

        Returns:
            QueryBuilder instance for chaining
        """
        self._parts.append(f"MERGE {pattern}")
        self._current_clause = "MERGE"
        return self

    def where_(self, condition: str) -> "QueryBuilder":
        """Add a WHERE clause.

        Args:
            condition: Where condition (e.g., "n.age > $min_age")

        Returns:
            QueryBuilder instance for chaining
        """
        self._parts.append(f"WHERE {condition}")
        self._current_clause = "WHERE"
        return self

    def return_(self, items: str) -> "QueryBuilder":
        """Add a RETURN clause.

        Args:
            items: Return items (e.g., "n.name, n.age")

        Returns:
            QueryBuilder instance for chaining
        """
        self._parts.append(f"RETURN {items}")
        self._current_clause = "RETURN"
        return self

    def set_(self, assignments: str) -> "QueryBuilder":
        """Add a SET clause.

        Args:
            assignments: Set assignments (e.g., "n.age = $age")

        Returns:
            QueryBuilder instance for chaining
        """
        self._parts.append(f"SET {assignments}")
        self._current_clause = "SET"
        return self

    def delete(self, items: str) -> "QueryBuilder":
        """Add a DELETE clause.

        Args:
            items: Items to delete (e.g., "n, r")

        Returns:
            QueryBuilder instance for chaining
        """
        self._parts.append(f"DELETE {items}")
        self._current_clause = "DELETE"
        return self

    def with_(self, items: str) -> "QueryBuilder":
        """Add a WITH clause.

        Args:
            items: With items (e.g., "n, count(*) AS cnt")

        Returns:
            QueryBuilder instance for chaining
        """
        self._parts.append(f"WITH {items}")
        self._current_clause = "WITH"
        return self

    def order_by(self, expression: str) -> "QueryBuilder":
        """Add an ORDER BY clause.

        Args:
            expression: Order expression (e.g., "n.age DESC")

        Returns:
            QueryBuilder instance for chaining
        """
        self._parts.append(f"ORDER BY {expression}")
        self._current_clause = "ORDER_BY"
        return self

    def limit(self, count: int) -> "QueryBuilder":
        """Add a LIMIT clause.

        Args:
            count: Limit count

        Returns:
            QueryBuilder instance for chaining
        """
        self._parts.append(f"LIMIT {count}")
        self._current_clause = "LIMIT"
        return self

    def skip(self, count: int) -> "QueryBuilder":
        """Add a SKIP clause.

        Args:
            count: Skip count

        Returns:
            QueryBuilder instance for chaining
        """
        self._parts.append(f"SKIP {count}")
        self._current_clause = "SKIP"
        return self

    def param(self, name: str, value: Any) -> "QueryBuilder":
        """Add a query parameter.

        Args:
            name: Parameter name (without $ prefix)
            value: Parameter value

        Returns:
            QueryBuilder instance for chaining
        """
        self._params[name] = value
        return self

    def build(self) -> "BuiltQuery":
        """Build the query.

        Returns:
            BuiltQuery object containing query string and parameters
        """
        query = " ".join(self._parts)
        params = self._params.copy() if self._params else None
        return BuiltQuery(query, params)

    def query(self) -> str:
        """Get the query string without building.

        Returns:
            Query string
        """
        return " ".join(self._parts)

    def params(self) -> Optional[Dict[str, Any]]:
        """Get the parameters without building.

        Returns:
            Parameters dictionary or None
        """
        return self._params.copy() if self._params else None

    def into_parts(self) -> Tuple[str, Optional[Dict[str, Any]]]:
        """Get query and parameters as separate parts.

        Returns:
            Tuple of (query_string, parameters_dict)
        """
        return (self.query(), self.params())


class BuiltQuery:
    """Built query with query string and parameters."""

    def __init__(self, query: str, params: Optional[Dict[str, Any]] = None):
        """Create a built query.

        Args:
            query: Cypher query string
            params: Optional parameters dictionary
        """
        self._query = query
        self._params = params

    @property
    def query(self) -> str:
        """Get the query string."""
        return self._query

    @property
    def params(self) -> Optional[Dict[str, Any]]:
        """Get the parameters."""
        return self._params

    def __str__(self) -> str:
        """String representation."""
        if self._params:
            return f"Query: {self._query}\nParams: {self._params}"
        return f"Query: {self._query}"

    def __repr__(self) -> str:
        """Representation."""
        return f"BuiltQuery(query={self._query!r}, params={self._params!r})"

