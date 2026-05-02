using System.Text.Json.Serialization;

namespace Nexus.SDK;

/// <summary>
/// Represents a graph node.
/// </summary>
public class Node
{
    /// <summary>
    /// Unique node identifier.
    /// </summary>
    [JsonPropertyName("id")]
    public string Id { get; set; } = string.Empty;

    /// <summary>
    /// Node labels.
    /// </summary>
    [JsonPropertyName("labels")]
    public List<string> Labels { get; set; } = new();

    /// <summary>
    /// Node properties.
    /// </summary>
    [JsonPropertyName("properties")]
    public Dictionary<string, object?> Properties { get; set; } = new();
}

/// <summary>
/// Represents a graph relationship.
/// </summary>
public class Relationship
{
    /// <summary>
    /// Unique relationship identifier.
    /// </summary>
    [JsonPropertyName("id")]
    public string Id { get; set; } = string.Empty;

    /// <summary>
    /// Relationship type.
    /// </summary>
    [JsonPropertyName("type")]
    public string Type { get; set; } = string.Empty;

    /// <summary>
    /// Start node ID.
    /// </summary>
    [JsonPropertyName("start_node")]
    public string StartNode { get; set; } = string.Empty;

    /// <summary>
    /// End node ID.
    /// </summary>
    [JsonPropertyName("end_node")]
    public string EndNode { get; set; } = string.Empty;

    /// <summary>
    /// Relationship properties.
    /// </summary>
    [JsonPropertyName("properties")]
    public Dictionary<string, object?> Properties { get; set; } = new();
}

/// <summary>
/// Represents the result of a Cypher query.
/// </summary>
public class QueryResult
{
    /// <summary>
    /// Column names in the result.
    /// </summary>
    [JsonPropertyName("columns")]
    public List<string> Columns { get; set; } = new();

    /// <summary>
    /// Result rows as arrays (Neo4j-compatible format).
    /// </summary>
    [JsonPropertyName("rows")]
    public List<List<object?>> Rows { get; set; } = new();

    /// <summary>
    /// Query execution statistics.
    /// </summary>
    [JsonPropertyName("stats")]
    public QueryStats? Stats { get; set; }

    /// <summary>
    /// Converts array-based rows to dictionary-based rows using column names as keys.
    /// </summary>
    public List<Dictionary<string, object?>> RowsAsMap()
    {
        var result = new List<Dictionary<string, object?>>();
        foreach (var row in Rows)
        {
            var rowDict = new Dictionary<string, object?>();
            for (int i = 0; i < Columns.Count && i < row.Count; i++)
            {
                rowDict[Columns[i]] = row[i];
            }
            result.Add(rowDict);
        }
        return result;
    }
}

/// <summary>
/// Contains execution statistics for a query.
/// </summary>
public class QueryStats
{
    /// <summary>
    /// Number of nodes created.
    /// </summary>
    [JsonPropertyName("nodes_created")]
    public int NodesCreated { get; set; }

    /// <summary>
    /// Number of nodes deleted.
    /// </summary>
    [JsonPropertyName("nodes_deleted")]
    public int NodesDeleted { get; set; }

    /// <summary>
    /// Number of relationships created.
    /// </summary>
    [JsonPropertyName("relationships_created")]
    public int RelationshipsCreated { get; set; }

    /// <summary>
    /// Number of relationships deleted.
    /// </summary>
    [JsonPropertyName("relationships_deleted")]
    public int RelationshipsDeleted { get; set; }

    /// <summary>
    /// Number of properties set.
    /// </summary>
    [JsonPropertyName("properties_set")]
    public int PropertiesSet { get; set; }

    /// <summary>
    /// Query execution time in milliseconds.
    /// </summary>
    [JsonPropertyName("execution_time_ms")]
    public double ExecutionTimeMs { get; set; }
}

/// <summary>
/// Represents a database index.
/// </summary>
public class Index
{
    /// <summary>
    /// Index name.
    /// </summary>
    [JsonPropertyName("name")]
    public string Name { get; set; } = string.Empty;

    /// <summary>
    /// Label the index is on.
    /// </summary>
    [JsonPropertyName("label")]
    public string Label { get; set; } = string.Empty;

    /// <summary>
    /// Properties included in the index.
    /// </summary>
    [JsonPropertyName("properties")]
    public List<string> Properties { get; set; } = new();

    /// <summary>
    /// Index type (e.g., btree, fulltext).
    /// </summary>
    [JsonPropertyName("type")]
    public string Type { get; set; } = string.Empty;
}

/// <summary>
/// Configuration for the Nexus client.
///
/// Transport precedence: URL scheme in <see cref="BaseUrl"/> &gt;
/// <c>NEXUS_SDK_TRANSPORT</c> env var &gt; <see cref="Transport"/>
/// field &gt; default (binary RPC).
/// </summary>
public class NexusClientConfig
{
    /// <summary>
    /// Endpoint URL. Accepts <c>nexus://</c> (binary RPC, default),
    /// <c>http://</c> / <c>https://</c>, <c>resp3://</c>, or bare
    /// <c>host[:port]</c>. Defaults to <c>nexus://127.0.0.1:15475</c>
    /// when empty.
    /// </summary>
    public string BaseUrl { get; set; } = "nexus://127.0.0.1:15475";

    /// <summary>
    /// API key for authentication (optional).
    /// </summary>
    public string? ApiKey { get; set; }

    /// <summary>
    /// Username for authentication (optional).
    /// </summary>
    public string? Username { get; set; }

    /// <summary>
    /// Password for authentication (optional).
    /// </summary>
    public string? Password { get; set; }

    /// <summary>
    /// HTTP request timeout (default: 30 seconds). Ignored by the RPC transport.
    /// </summary>
    public TimeSpan Timeout { get; set; } = TimeSpan.FromSeconds(30);

    /// <summary>
    /// Explicit transport hint. URL scheme wins if set.
    /// </summary>
    public Nexus.SDK.Transports.TransportMode? Transport { get; set; }

    /// <summary>RPC port override (default 15475).</summary>
    public ushort? RpcPort { get; set; }

    /// <summary>RESP3 port override (default 15476).</summary>
    public ushort? Resp3Port { get; set; }
}

/// <summary>
/// Request body for POST /data/nodes (create node with optional external id).
/// </summary>
public class CreateNodeRequest
{
    /// <summary>Node labels.</summary>
    [JsonPropertyName("labels")]
    public List<string> Labels { get; set; } = new();

    /// <summary>Node properties.</summary>
    [JsonPropertyName("properties")]
    public Dictionary<string, object?> Properties { get; set; } = new();

    /// <summary>
    /// Optional caller-supplied external id in prefixed string form:
    /// <c>sha256:&lt;hex&gt;</c>, <c>blake3:&lt;hex&gt;</c>, <c>sha512:&lt;hex&gt;</c>,
    /// <c>uuid:&lt;canonical&gt;</c>, <c>str:&lt;utf8&gt;</c>, <c>bytes:&lt;hex&gt;</c>.
    /// Omitted when null.
    /// </summary>
    [JsonPropertyName("external_id")]
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? ExternalId { get; set; }

    /// <summary>
    /// Optional conflict policy when <see cref="ExternalId"/> is set:
    /// <c>"error"</c> (default), <c>"match"</c>, or <c>"replace"</c>.
    /// Omitted when null.
    /// </summary>
    [JsonPropertyName("conflict_policy")]
    [JsonIgnore(Condition = JsonIgnoreCondition.WhenWritingNull)]
    public string? ConflictPolicy { get; set; }
}

/// <summary>
/// Response from POST /data/nodes.
/// </summary>
public class CreateNodeResponse
{
    /// <summary>Created node ID.</summary>
    [JsonPropertyName("node_id")]
    public ulong NodeId { get; set; }

    /// <summary>Success message.</summary>
    [JsonPropertyName("message")]
    public string Message { get; set; } = string.Empty;

    /// <summary>Error message, if any.</summary>
    [JsonPropertyName("error")]
    public string? Error { get; set; }
}

/// <summary>
/// Response from GET /data/nodes/by-external-id.
/// </summary>
public class GetNodeByExternalIdResponse
{
    /// <summary>
    /// The matched node, or <see langword="null"/> when no node was found.
    /// </summary>
    [JsonPropertyName("node")]
    public Node? Node { get; set; }

    /// <summary>Status message from the server.</summary>
    [JsonPropertyName("message")]
    public string Message { get; set; } = string.Empty;

    /// <summary>Error message, if any.</summary>
    [JsonPropertyName("error")]
    public string? Error { get; set; }
}

/// <summary>
/// Response from <c>GET /data/nodes?id=&lt;id&gt;</c>. The server
/// returns 200 with <c>node: null</c> when the id is absent — callers
/// check <see cref="Node"/> rather than catching for missing ids.
/// Phase 11 §2.5.
/// </summary>
public class GetNodeResponse
{
    /// <summary>The matched node, or null when the id is absent.</summary>
    [JsonPropertyName("node")]
    public Node? Node { get; set; }

    /// <summary>Status message from the server.</summary>
    [JsonPropertyName("message")]
    public string Message { get; set; } = string.Empty;

    /// <summary>Error message, if any.</summary>
    [JsonPropertyName("error")]
    public string? Error { get; set; }
}

/// <summary>
/// Request body for batch node creation.
/// </summary>
public class BatchNodesRequest
{
    /// <summary>
    /// Nodes to create.
    /// </summary>
    [JsonPropertyName("nodes")]
    public List<NodeInput> Nodes { get; set; } = new();
}

/// <summary>
/// Input for creating a node.
/// </summary>
public class NodeInput
{
    /// <summary>
    /// Node labels.
    /// </summary>
    [JsonPropertyName("labels")]
    public List<string> Labels { get; set; } = new();

    /// <summary>
    /// Node properties.
    /// </summary>
    [JsonPropertyName("properties")]
    public Dictionary<string, object?> Properties { get; set; } = new();
}

/// <summary>
/// Request body for batch relationship creation.
/// </summary>
public class BatchRelationshipsRequest
{
    /// <summary>
    /// Relationships to create.
    /// </summary>
    [JsonPropertyName("relationships")]
    public List<RelationshipInput> Relationships { get; set; } = new();
}

/// <summary>
/// Input for creating a relationship.
/// </summary>
public class RelationshipInput
{
    /// <summary>
    /// Start node ID.
    /// </summary>
    [JsonPropertyName("start_node")]
    public string StartNode { get; set; } = string.Empty;

    /// <summary>
    /// End node ID.
    /// </summary>
    [JsonPropertyName("end_node")]
    public string EndNode { get; set; } = string.Empty;

    /// <summary>
    /// Relationship type.
    /// </summary>
    [JsonPropertyName("type")]
    public string Type { get; set; } = string.Empty;

    /// <summary>
    /// Relationship properties.
    /// </summary>
    [JsonPropertyName("properties")]
    public Dictionary<string, object?> Properties { get; set; } = new();
}

/// <summary>
/// One entry in the response of <c>GET /schema/labels</c>.
/// </summary>
/// <remarks>
/// Wire shape: <c>{"name": "Person", "id": 0}</c>. The <c>Id</c>
/// field is the catalog id allocated by the engine, not a count.
/// Renamed from a JSON tuple <c>["Person", 0]</c> in nexus-server
/// 1.15+ - see issue
/// <a href="https://github.com/hivellm/nexus/issues/2">hivellm/nexus#2</a>.
/// </remarks>
public class LabelInfo
{
    /// <summary>Label name as registered in the engine catalog.</summary>
    [JsonPropertyName("name")]
    public string Name { get; set; } = string.Empty;

    /// <summary>Catalog id allocated to this label.</summary>
    [JsonPropertyName("id")]
    public uint Id { get; set; }
}

/// <summary>
/// One entry in the response of <c>GET /schema/rel_types</c>. Mirrors
/// <see cref="LabelInfo"/>.
/// </summary>
public class RelTypeInfo
{
    /// <summary>Relationship type name as registered in the catalog.</summary>
    [JsonPropertyName("name")]
    public string Name { get; set; } = string.Empty;

    /// <summary>Catalog id allocated to this relationship type.</summary>
    [JsonPropertyName("id")]
    public uint Id { get; set; }
}
