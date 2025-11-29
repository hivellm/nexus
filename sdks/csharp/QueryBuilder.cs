using System.Text;

namespace Nexus.SDK;

/// <summary>
/// Fluent API for constructing Cypher queries.
/// </summary>
public class QueryBuilder
{
    private readonly List<string> _matchClauses = new();
    private readonly List<string> _whereClauses = new();
    private readonly List<string> _createClauses = new();
    private readonly List<string> _setClauses = new();
    private readonly List<string> _deleteClauses = new();
    private readonly List<string> _returnClauses = new();
    private readonly List<string> _orderByClauses = new();
    private int? _skipValue;
    private int? _limitValue;
    private readonly Dictionary<string, object?> _parameters = new();

    /// <summary>
    /// Creates a new QueryBuilder instance.
    /// </summary>
    public static QueryBuilder Create() => new();

    /// <summary>
    /// Adds a MATCH clause to the query.
    /// </summary>
    public QueryBuilder Match(string pattern)
    {
        _matchClauses.Add(pattern);
        return this;
    }

    /// <summary>
    /// Adds an OPTIONAL MATCH clause to the query.
    /// </summary>
    public QueryBuilder OptionalMatch(string pattern)
    {
        _matchClauses.Add($"OPTIONAL MATCH {pattern}");
        return this;
    }

    /// <summary>
    /// Adds a WHERE clause to the query.
    /// </summary>
    public QueryBuilder Where(string condition)
    {
        _whereClauses.Add(condition);
        return this;
    }

    /// <summary>
    /// Adds an AND condition to the WHERE clause.
    /// </summary>
    public QueryBuilder And(string condition)
    {
        if (_whereClauses.Count > 0)
        {
            _whereClauses[^1] += $" AND {condition}";
        }
        else
        {
            _whereClauses.Add(condition);
        }
        return this;
    }

    /// <summary>
    /// Adds an OR condition to the WHERE clause.
    /// </summary>
    public QueryBuilder Or(string condition)
    {
        if (_whereClauses.Count > 0)
        {
            _whereClauses[^1] += $" OR {condition}";
        }
        else
        {
            _whereClauses.Add(condition);
        }
        return this;
    }

    /// <summary>
    /// Adds a CREATE clause to the query.
    /// </summary>
    public QueryBuilder CreatePattern(string pattern)
    {
        _createClauses.Add(pattern);
        return this;
    }

    /// <summary>
    /// Adds a MERGE clause to the query.
    /// </summary>
    public QueryBuilder Merge(string pattern)
    {
        _createClauses.Add($"MERGE {pattern}");
        return this;
    }

    /// <summary>
    /// Adds a SET clause to the query.
    /// </summary>
    public QueryBuilder Set(string assignment)
    {
        _setClauses.Add(assignment);
        return this;
    }

    /// <summary>
    /// Adds a DELETE clause to the query.
    /// </summary>
    public QueryBuilder Delete(string items)
    {
        _deleteClauses.Add(items);
        return this;
    }

    /// <summary>
    /// Adds a DETACH DELETE clause to the query.
    /// </summary>
    public QueryBuilder DetachDelete(string items)
    {
        _deleteClauses.Add($"DETACH DELETE {items}");
        return this;
    }

    /// <summary>
    /// Adds a RETURN clause to the query.
    /// </summary>
    public QueryBuilder Return(params string[] items)
    {
        _returnClauses.AddRange(items);
        return this;
    }

    /// <summary>
    /// Adds a RETURN DISTINCT clause to the query.
    /// </summary>
    public QueryBuilder ReturnDistinct(params string[] items)
    {
        if (_returnClauses.Count == 0)
        {
            _returnClauses.Add($"DISTINCT {string.Join(", ", items)}");
        }
        else
        {
            _returnClauses.AddRange(items);
        }
        return this;
    }

    /// <summary>
    /// Adds an ORDER BY clause to the query.
    /// </summary>
    public QueryBuilder OrderBy(params string[] items)
    {
        _orderByClauses.AddRange(items);
        return this;
    }

    /// <summary>
    /// Adds an ORDER BY ... DESC clause to the query.
    /// </summary>
    public QueryBuilder OrderByDesc(string item)
    {
        _orderByClauses.Add($"{item} DESC");
        return this;
    }

    /// <summary>
    /// Adds a SKIP clause to the query.
    /// </summary>
    public QueryBuilder Skip(int n)
    {
        _skipValue = n;
        return this;
    }

    /// <summary>
    /// Adds a LIMIT clause to the query.
    /// </summary>
    public QueryBuilder Limit(int n)
    {
        _limitValue = n;
        return this;
    }

    /// <summary>
    /// Adds a parameter to the query.
    /// </summary>
    public QueryBuilder WithParam(string name, object? value)
    {
        _parameters[name] = value;
        return this;
    }

    /// <summary>
    /// Adds multiple parameters to the query.
    /// </summary>
    public QueryBuilder WithParams(Dictionary<string, object?> parameters)
    {
        foreach (var (key, value) in parameters)
        {
            _parameters[key] = value;
        }
        return this;
    }

    /// <summary>
    /// Builds the final Cypher query string.
    /// </summary>
    public string Build()
    {
        var parts = new List<string>();

        // MATCH clauses
        foreach (var match in _matchClauses)
        {
            if (match.StartsWith("OPTIONAL MATCH"))
            {
                parts.Add(match);
            }
            else
            {
                parts.Add($"MATCH {match}");
            }
        }

        // WHERE clauses
        if (_whereClauses.Count > 0)
        {
            parts.Add($"WHERE {string.Join(" AND ", _whereClauses)}");
        }

        // CREATE/MERGE clauses
        foreach (var create in _createClauses)
        {
            if (create.StartsWith("MERGE"))
            {
                parts.Add(create);
            }
            else
            {
                parts.Add($"CREATE {create}");
            }
        }

        // SET clauses
        if (_setClauses.Count > 0)
        {
            parts.Add($"SET {string.Join(", ", _setClauses)}");
        }

        // DELETE clauses
        foreach (var del in _deleteClauses)
        {
            if (del.StartsWith("DETACH DELETE"))
            {
                parts.Add(del);
            }
            else
            {
                parts.Add($"DELETE {del}");
            }
        }

        // RETURN clause
        if (_returnClauses.Count > 0)
        {
            parts.Add($"RETURN {string.Join(", ", _returnClauses)}");
        }

        // ORDER BY clause
        if (_orderByClauses.Count > 0)
        {
            parts.Add($"ORDER BY {string.Join(", ", _orderByClauses)}");
        }

        // SKIP clause
        if (_skipValue.HasValue)
        {
            parts.Add($"SKIP {_skipValue}");
        }

        // LIMIT clause
        if (_limitValue.HasValue)
        {
            parts.Add($"LIMIT {_limitValue}");
        }

        return string.Join(" ", parts);
    }

    /// <summary>
    /// Gets the parameters for the query.
    /// </summary>
    public Dictionary<string, object?> Parameters => _parameters;
}

/// <summary>
/// Builder for node patterns in Cypher queries.
/// </summary>
public class NodePatternBuilder
{
    private string _variable = "";
    private readonly List<string> _labels = new();
    private readonly Dictionary<string, object?> _properties = new();

    /// <summary>
    /// Creates a new NodePatternBuilder.
    /// </summary>
    public static NodePatternBuilder Create(string variable = "")
    {
        return new NodePatternBuilder { _variable = variable };
    }

    /// <summary>
    /// Sets the variable name.
    /// </summary>
    public NodePatternBuilder Variable(string variable)
    {
        _variable = variable;
        return this;
    }

    /// <summary>
    /// Adds a label to the node.
    /// </summary>
    public NodePatternBuilder WithLabel(string label)
    {
        _labels.Add(label);
        return this;
    }

    /// <summary>
    /// Adds multiple labels to the node.
    /// </summary>
    public NodePatternBuilder WithLabels(params string[] labels)
    {
        _labels.AddRange(labels);
        return this;
    }

    /// <summary>
    /// Adds a property to the node.
    /// </summary>
    public NodePatternBuilder WithProperty(string key, object? value)
    {
        _properties[key] = value;
        return this;
    }

    /// <summary>
    /// Adds multiple properties to the node.
    /// </summary>
    public NodePatternBuilder WithProperties(Dictionary<string, object?> properties)
    {
        foreach (var (key, value) in properties)
        {
            _properties[key] = value;
        }
        return this;
    }

    /// <summary>
    /// Builds the node pattern string.
    /// </summary>
    public string Build()
    {
        var sb = new StringBuilder();
        sb.Append('(');
        sb.Append(_variable);

        foreach (var label in _labels)
        {
            sb.Append(':');
            sb.Append(label);
        }

        if (_properties.Count > 0)
        {
            sb.Append(" {");
            var first = true;
            foreach (var (key, value) in _properties)
            {
                if (!first) sb.Append(", ");
                sb.Append(key);
                sb.Append(": ");
                sb.Append(FormatValue(value));
                first = false;
            }
            sb.Append('}');
        }

        sb.Append(')');
        return sb.ToString();
    }

    /// <summary>
    /// Implicit conversion to string.
    /// </summary>
    public static implicit operator string(NodePatternBuilder builder) => builder.Build();

    private static string FormatValue(object? value)
    {
        return value switch
        {
            null => "null",
            string s => $"'{s.Replace("'", "\\'")}'",
            bool b => b ? "true" : "false",
            _ => value.ToString() ?? "null"
        };
    }
}

/// <summary>
/// Builder for relationship patterns in Cypher queries.
/// </summary>
public class RelationshipPatternBuilder
{
    private string _variable = "";
    private string _type = "";
    private RelationshipDirection _direction = RelationshipDirection.Outgoing;
    private int? _minHops;
    private int? _maxHops;

    /// <summary>
    /// Creates a new RelationshipPatternBuilder.
    /// </summary>
    public static RelationshipPatternBuilder Create(string variable = "")
    {
        return new RelationshipPatternBuilder { _variable = variable };
    }

    /// <summary>
    /// Sets the variable name.
    /// </summary>
    public RelationshipPatternBuilder Variable(string variable)
    {
        _variable = variable;
        return this;
    }

    /// <summary>
    /// Sets the relationship type.
    /// </summary>
    public RelationshipPatternBuilder WithType(string type)
    {
        _type = type;
        return this;
    }

    /// <summary>
    /// Sets the direction to outgoing (->).
    /// </summary>
    public RelationshipPatternBuilder Outgoing()
    {
        _direction = RelationshipDirection.Outgoing;
        return this;
    }

    /// <summary>
    /// Sets the direction to incoming (<-).
    /// </summary>
    public RelationshipPatternBuilder Incoming()
    {
        _direction = RelationshipDirection.Incoming;
        return this;
    }

    /// <summary>
    /// Sets the relationship to undirected (-).
    /// </summary>
    public RelationshipPatternBuilder Undirected()
    {
        _direction = RelationshipDirection.Undirected;
        return this;
    }

    /// <summary>
    /// Sets variable length path hops.
    /// </summary>
    public RelationshipPatternBuilder WithHops(int min, int max)
    {
        _minHops = min;
        _maxHops = max;
        return this;
    }

    /// <summary>
    /// Sets minimum hops for variable length path.
    /// </summary>
    public RelationshipPatternBuilder WithMinHops(int min)
    {
        _minHops = min;
        return this;
    }

    /// <summary>
    /// Sets maximum hops for variable length path.
    /// </summary>
    public RelationshipPatternBuilder WithMaxHops(int max)
    {
        _maxHops = max;
        return this;
    }

    /// <summary>
    /// Builds the relationship pattern string.
    /// </summary>
    public string Build()
    {
        var sb = new StringBuilder();

        // Start arrow
        if (_direction == RelationshipDirection.Incoming)
        {
            sb.Append("<-[");
        }
        else
        {
            sb.Append("-[");
        }

        sb.Append(_variable);

        if (!string.IsNullOrEmpty(_type))
        {
            sb.Append(':');
            sb.Append(_type);
        }

        // Variable length
        if (_minHops.HasValue || _maxHops.HasValue)
        {
            sb.Append('*');
            if (_minHops.HasValue)
            {
                sb.Append(_minHops);
            }
            sb.Append("..");
            if (_maxHops.HasValue)
            {
                sb.Append(_maxHops);
            }
        }

        sb.Append("]-");

        // End arrow
        if (_direction == RelationshipDirection.Outgoing)
        {
            sb.Append('>');
        }

        return sb.ToString();
    }

    /// <summary>
    /// Implicit conversion to string.
    /// </summary>
    public static implicit operator string(RelationshipPatternBuilder builder) => builder.Build();
}

/// <summary>
/// Direction of a relationship in a Cypher pattern.
/// </summary>
public enum RelationshipDirection
{
    /// <summary>Outgoing relationship (->)</summary>
    Outgoing,
    /// <summary>Incoming relationship (<-)</summary>
    Incoming,
    /// <summary>Undirected relationship (-)</summary>
    Undirected
}

/// <summary>
/// Helper class for building path patterns.
/// </summary>
public static class PathBuilder
{
    /// <summary>
    /// Combines patterns into a path.
    /// </summary>
    public static string Path(params string[] patterns)
    {
        return string.Concat(patterns);
    }
}
