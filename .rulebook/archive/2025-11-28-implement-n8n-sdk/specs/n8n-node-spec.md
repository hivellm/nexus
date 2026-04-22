# n8n Node Specification for Nexus

## Node Definition

### Basic Information

- **Node Type**: `@hivellm/nexus`
- **Node Version**: `1.0.0`
- **Node Name**: Nexus
- **Node Description**: Execute graph operations on Nexus graph database
- **Node Icon**: Graph database icon
- **Node Category**: Database
- **Node Color**: `#1f77b4` (blue)

### Node Properties

#### Operation Selection

- **Property Name**: `operation`
- **Property Type**: Options
- **Required**: Yes
- **Options**:
  - `executeCypher` - Execute Cypher Query
  - `createNode` - Create Node
  - `readNode` - Read Node
  - `updateNode` - Update Node
  - `deleteNode` - Delete Node
  - `createRelationship` - Create Relationship
  - `readRelationship` - Read Relationship
  - `updateRelationship` - Update Relationship
  - `deleteRelationship` - Delete Relationship
  - `batchCreateNodes` - Batch Create Nodes
  - `batchCreateRelationships` - Batch Create Relationships
  - `listLabels` - List Labels
  - `listRelationshipTypes` - List Relationship Types
  - `shortestPath` - Find Shortest Path
  - `graphAlgorithm` - Execute Graph Algorithm

## Operation Specifications

### Execute Cypher Query

**Operation**: `executeCypher`

**Properties**:
- `query` (string, required) - Cypher query to execute
- `parameters` (object, optional) - Query parameters
- `transformResults` (boolean, optional) - Transform results to flat format
- `returnFormat` (options, optional) - Result format: `json` | `table` | `graph`

**Output**:
```json
{
  "columns": ["name", "age"],
  "rows": [
    ["Alice", 30],
    ["Bob", 25]
  ],
  "executionTime": 3
}
```

### Create Node

**Operation**: `createNode`

**Properties**:
- `labels` (array, required) - Node labels
- `properties` (object, required) - Node properties
- `returnNode` (boolean, optional) - Return created node

**Output**:
```json
{
  "id": 123,
  "labels": ["Person"],
  "properties": {
    "name": "Alice",
    "age": 30
  }
}
```

### Read Node

**Operation**: `readNode`

**Properties**:
- `nodeId` (number, required) - Node ID
- `returnProperties` (boolean, optional) - Return node properties
- `returnLabels` (boolean, optional) - Return node labels

**Output**:
```json
{
  "id": 123,
  "labels": ["Person"],
  "properties": {
    "name": "Alice",
    "age": 30
  }
}
```

### Update Node

**Operation**: `updateNode`

**Properties**:
- `nodeId` (number, required) - Node ID
- `properties` (object, required) - Properties to update
- `addLabels` (array, optional) - Labels to add
- `removeLabels` (array, optional) - Labels to remove

**Output**:
```json
{
  "id": 123,
  "labels": ["Person", "Employee"],
  "properties": {
    "name": "Alice",
    "age": 31,
    "department": "Engineering"
  }
}
```

### Delete Node

**Operation**: `deleteNode`

**Properties**:
- `nodeId` (number, required) - Node ID
- `deleteRelationships` (boolean, optional) - Delete connected relationships

**Output**:
```json
{
  "deleted": true,
  "nodeId": 123
}
```

### Create Relationship

**Operation**: `createRelationship`

**Properties**:
- `sourceNodeId` (number, required) - Source node ID
- `targetNodeId` (number, required) - Target node ID
- `relationshipType` (string, required) - Relationship type
- `properties` (object, optional) - Relationship properties
- `returnRelationship` (boolean, optional) - Return created relationship

**Output**:
```json
{
  "id": 456,
  "type": "KNOWS",
  "from": 123,
  "to": 124,
  "properties": {
    "since": "2020-01-01"
  }
}
```

### Batch Create Nodes

**Operation**: `batchCreateNodes`

**Properties**:
- `nodes` (array, required) - Array of node definitions
- `batchSize` (number, optional) - Batch size (default: 100)
- `returnNodes` (boolean, optional) - Return created nodes

**Node Definition**:
```json
{
  "labels": ["Person"],
  "properties": {
    "name": "Alice",
    "age": 30
  }
}
```

**Output**:
```json
{
  "created": 100,
  "nodes": [...]
}
```

### Batch Create Relationships

**Operation**: `batchCreateRelationships`

**Properties**:
- `relationships` (array, required) - Array of relationship definitions
- `batchSize` (number, optional) - Batch size (default: 100)
- `returnRelationships` (boolean, optional) - Return created relationships

**Relationship Definition**:
```json
{
  "from": 123,
  "to": 124,
  "type": "KNOWS",
  "properties": {
    "since": "2020-01-01"
  }
}
```

**Output**:
```json
{
  "created": 100,
  "relationships": [...]
}
```

### Shortest Path

**Operation**: `shortestPath`

**Properties**:
- `startNodeId` (number, required) - Start node ID
- `endNodeId` (number, required) - End node ID
- `relationshipTypes` (array, optional) - Filter by relationship types
- `maxDepth` (number, optional) - Maximum path depth

**Output**:
```json
{
  "path": [123, 125, 127, 128],
  "length": 3,
  "relationships": [456, 457, 458]
}
```

## Credential Specification

### API Key Credential

**Credential Type**: `nexusApi`

**Properties**:
- `apiKey` (string, required) - Nexus API key
- `host` (string, required) - Nexus server host
- `port` (number, optional) - Nexus server port (default: 15474)
- `useTls` (boolean, optional) - Use TLS/HTTPS (default: false)

### User/Password Credential

**Credential Type**: `nexusUser`

**Properties**:
- `username` (string, required) - Nexus username
- `password` (string, required) - Nexus password
- `host` (string, required) - Nexus server host
- `port` (number, optional) - Nexus server port (default: 15474)
- `useTls` (boolean, optional) - Use TLS/HTTPS (default: false)

## Error Handling

### Error Types

1. **ConnectionError**: Failed to connect to Nexus server
2. **AuthenticationError**: Invalid credentials
3. **QueryError**: Cypher query execution error
4. **ValidationError**: Invalid input parameters
5. **NotFoundError**: Node/relationship not found
6. **RateLimitError**: Rate limit exceeded

### Error Response Format

```json
{
  "error": {
    "type": "QueryError",
    "message": "Syntax error in Cypher query",
    "details": {
      "line": 1,
      "column": 10,
      "query": "MATCH (n) RETRUN n"
    }
  }
}
```

## Result Transformation

### Transform Options

1. **Flatten**: Flatten nested objects
2. **Table**: Convert to table format
3. **Graph**: Convert to graph format
4. **Custom**: Apply custom transformation expression

### Example Transformations

**Flatten**:
```json
{
  "input": {
    "person": {
      "name": "Alice",
      "age": 30
    }
  },
  "output": {
    "person.name": "Alice",
    "person.age": 30
  }
}
```

**Table**:
```json
{
  "columns": ["name", "age"],
  "rows": [
    ["Alice", 30],
    ["Bob", 25]
  ]
}
```

## Testing Requirements

### Unit Tests

- Test all operation implementations
- Test error handling
- Test result transformations
- Test credential management
- ≥90% code coverage

### Integration Tests

- Test with real Nexus server
- Test all operations end-to-end
- Test error scenarios
- Test credential validation

### n8n Compatibility Tests

- Test with n8n v1.x
- Test node loading
- Test workflow execution
- Test error handling in workflows

