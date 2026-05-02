# External Node IDs Guide

External IDs enable idempotent, deterministic data ingestion by allowing you to assign caller-supplied identifiers to nodes alongside Nexus's internal IDs. This guide covers motivating use cases and practical examples.

## Motivation: Why External IDs?

### File-Hash Ingestion (Idempotency)

When ingesting files, their content hash is a stable, deterministic identifier:

```cypher
-- First import
CREATE (f:File {_id: 'sha256:abc123…', path: '/data/file.txt', size: 1024})

-- Re-import same file (no duplicate created)
CREATE (f:File {_id: 'sha256:abc123…', path: '/data/file.txt', size: 1024}) ON CONFLICT MATCH
RETURN f._id, f.path
```

**Result**: No duplicates. The same file hash always maps to the same node, even across multiple runs.

### Deterministic Re-Import & Disaster Recovery

After a failure, reimporting data from an external source reproduces the exact same graph:

```cypher
-- Original import
CREATE (doc:Document {_id: 'uuid:550e8400-…', title: 'Report', created: 2025-04-30})
CREATE (user:User {_id: 'uuid:7c2e6d8a-…', name: 'Alice'})
CREATE (doc)<-[:WRITTEN_BY]-(user)

-- Disaster recovery: re-run the same import script
-- → same uuid external ids recreate the same nodes
-- → graph topology is preserved
```

**Benefit**: Backup/restore and replica promotion use the same import path, guaranteed consistency.

### Cross-System Joins

When system A and system B share a logical identifier (e.g., a document UUID), both can reference the same Nexus node without a side table:

```cypher
-- System A imports documents
CREATE (doc:Document {_id: 'uuid:12345678-…', title: 'Design Doc'})

-- System B independently ingests the same document
MATCH (doc:Document {_id: 'uuid:12345678-…'})
-- Both systems see the same node; no mapping table needed
CREATE (analysis:Analysis {content: '...'})
CREATE (analysis)-[:FOR]->(doc)
```

## Conflict Policies in Practice

### ERROR (Defensive Create)

Use when creating is expected to succeed with a unique external id:

```cypher
CREATE (n:Node {_id: 'unique:abc', data: 'value'})
RETURN n._id
-- Fails with ExternalIdConflict if 'unique:abc' already exists
-- Useful for validation: ensures no accidental duplicates
```

### MATCH (Idempotent Ingest)

Use for idempotent batch imports where the same data may be applied multiple times:

```cypher
CREATE (f:File {_id: 'sha256:content…', path: '/file.txt'}) ON CONFLICT MATCH
CREATE (event:Event {_id: 'str:event-42', ts: 1234567890}) ON CONFLICT MATCH
-- Returns existing node unchanged; new properties discarded
```

**Typical workflow**:
```bash
# Day 1: import 1M files
nexus-ingest --conflict=match files.csv

# Day 2: re-run same import (some new, some duplicates)
nexus-ingest --conflict=match files.csv
# → existing nodes untouched, new files created
```

### REPLACE (Full Re-Sync)

Use when you want to update properties while preserving identity:

```cypher
CREATE (doc:Document {
  _id: 'uuid:550e8400-…',
  title: 'Updated Title',
  version: 2,
  updated_at: timestamp()
}) ON CONFLICT REPLACE
RETURN doc._id, doc.title, doc.version
-- Updates properties, keeps internal node id, labels unchanged
```

**Typical workflow**:
```cypher
-- Initial import
CREATE (config:Config {_id: 'str:prod-config', value: 'old'})

-- Later: full re-sync with new values
CREATE (config:Config {_id: 'str:prod-config', value: 'new'}) ON CONFLICT REPLACE
-- Properties updated, same internal id
```

## External ID Formats

### Hash Variants

**SHA-256** (32 bytes):
```cypher
CREATE (f:File {_id: 'sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'})
```

**SHA-512** (64 bytes):
```cypher
CREATE (f:File {_id: 'sha512:cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e'})
```

**BLAKE3** (32 bytes):
```cypher
CREATE (f:File {_id: 'blake3:d7a8fbb307d7809469ca9abdcbed9e5104cc07ff76718b6491f745474949df5e'})
```

### UUID

```cypher
CREATE (doc:Document {_id: 'uuid:550e8400-e29b-41d4-a716-446655440000'})
```

### String Keys

For arbitrary natural keys (document path, URN, SKU):

```cypher
CREATE (user:User {_id: 'str:user-12345', name: 'Alice'})
CREATE (item:Item {_id: 'str:sku-AB-001', price: 29.99})
CREATE (service:Service {_id: 'str:service:auth:v2', endpoint: '...'})
```

**Note**: Max 256 bytes UTF-8.

### Bytes

For opaque binary identifiers:

```cypher
CREATE (blob:Blob {_id: 'bytes:deadbeefcafebabe', content: $binary_data})
```

**Note**: Max 64 bytes; represented as hex in queries.

## Code Examples

### REST API

```bash
# Create node with external id
curl -X POST http://localhost:15474/data/nodes \
  -H "Content-Type: application/json" \
  -d '{
    "labels": ["File"],
    "properties": {
      "path": "/data/document.pdf",
      "size": 2048,
      "mime": "application/pdf"
    },
    "external_id": "sha256:abc123def456…",
    "conflict_policy": "match"
  }'

# Lookup node by external id
curl http://localhost:15474/data/nodes/by-external-id?external_id=sha256:abc123def456…
```

### Cypher

```cypher
-- Create with external id
CREATE (f:File {_id: 'sha256:abc…', path: '/file.txt'})
RETURN f._id, f.path

-- Idempotent create
CREATE (doc:Doc {_id: 'uuid:550e8400-…', title: 'Report'}) ON CONFLICT MATCH
RETURN doc._id

-- Update on conflict
CREATE (config:Config {_id: 'str:prod', value: 'new-value'}) ON CONFLICT REPLACE
RETURN config._id, config.value

-- Query by external id (index seek)
MATCH (n {_id: 'sha256:abc…'})
RETURN n

-- Project external id
MATCH (f:File)
RETURN f._id, f.path
-- Rows: ["sha256:abc…", "/file1.txt"], [null, "/file2.txt"], …
```

### Rust SDK

```rust
use nexus_sdk::{NexusClient, CreateNodeRequest};

#[tokio::main]
async fn main() {
    let client = NexusClient::new("http://localhost:15474");

    // Create with external id
    let req = CreateNodeRequest {
        labels: vec!["File".to_string()],
        properties: serde_json::json!({
            "path": "/data/file.txt",
            "size": 1024
        }),
        external_id: Some("sha256:abc123…".to_string()),
        conflict_policy: Some("match".to_string()),
    };

    let response = client.create_node(req).await.unwrap();
    println!("Node id: {}, external_id: {}", 
             response.node.id, response.node.external_id.unwrap());

    // Lookup by external id
    let node = client.get_node_by_external_id("sha256:abc123…").await.unwrap();
    println!("Found: {:?}", node);
}
```

### Python SDK

```python
from nexus_sdk import NexusClient

client = NexusClient("http://localhost:15474")

# Create with external id
response = client.create_node(
    labels=["File"],
    properties={
        "path": "/data/file.txt",
        "size": 1024
    },
    external_id="sha256:abc123…",
    conflict_policy="match"
)
print(f"Node: {response.node.id}, external_id: {response.node.external_id}")

# Lookup by external id
node = client.get_node_by_external_id("sha256:abc123…")
print(f"Found: {node}")

# Query with Cypher
result = client.execute_cypher(
    "MATCH (n {_id: $ext_id}) RETURN n._id, n.path",
    params={"ext_id": "sha256:abc123…"}
)
for row in result.rows:
    print(f"External ID: {row[0]}, Path: {row[1]}")
```

### TypeScript SDK

```typescript
import { NexusClient } from "@hivehub/nexus-sdk";

const client = new NexusClient("http://localhost:15474");

// Create with external id
const response = await client.createNode({
  labels: ["File"],
  properties: {
    path: "/data/file.txt",
    size: 1024,
  },
  external_id: "sha256:abc123…",
  conflict_policy: "match",
});

console.log(`Node: ${response.node.id}, external_id: ${response.node.external_id}`);

// Lookup by external id
const node = await client.getNodeByExternalId("sha256:abc123…");
console.log(`Found:`, node);

// Query with Cypher
const result = await client.executeQuery(
  "MATCH (n {_id: $ext_id}) RETURN n._id, n.path",
  { ext_id: "sha256:abc123…" }
);

for (const row of result.rows) {
  console.log(`External ID: ${row[0]}, Path: ${row[1]}`);
}
```

### Go SDK

```go
package main

import (
    "context"
    "fmt"
    "log"

    nexus "github.com/hivellm/nexus-go"
)

func main() {
    ctx := context.Background()
    client := nexus.NewClient(nexus.Config{BaseURL: "http://localhost:15474"})

    // Create with external id (idempotent on rerun via match policy)
    resp, err := client.CreateNodeWithExternalID(
        ctx,
        []string{"File"},
        map[string]interface{}{"path": "/data/file.txt", "size": 1024},
        "sha256:abc123",
        "match",
    )
    if err != nil { log.Fatal(err) }
    fmt.Printf("internal id: %d\n", resp.NodeID)

    // Resolve by external id
    got, err := client.GetNodeByExternalID(ctx, "sha256:abc123")
    if err != nil { log.Fatal(err) }
    if got.Node != nil {
        fmt.Printf("found id=%d labels=%v\n", got.Node.ID, got.Node.Labels)
    }
}
```

Pulled verbatim from `sdks/go/test/external_id_live_test.go` (15/15 live).

### C# SDK

```csharp
using Nexus.SDK;

var client = new NexusClient(new NexusConfig { BaseUrl = "http://localhost:15474" });

// Create with external id + conflict policy
var resp = await client.CreateNodeWithExternalIdAsync(
    labels: new[] { "File" },
    properties: new Dictionary<string, object?> { { "path", "/data/file.txt" } },
    externalId: "sha256:abc123",
    conflictPolicy: "match"
);
Console.WriteLine($"internal id: {resp.NodeId}");

// Resolve by external id
var got = await client.GetNodeByExternalIdAsync("sha256:abc123");
if (got.Node != null)
{
    Console.WriteLine($"found id={got.Node.Id} labels={string.Join(",", got.Node.Labels)}");
}
```

Pulled verbatim from `sdks/csharp/Tests/ExternalIdLiveTests.cs` (14/14 live).

### PHP SDK

```php
<?php
require __DIR__ . '/vendor/autoload.php';

use Nexus\SDK\NexusClient;
use Nexus\SDK\Config\Config;

$client = new NexusClient(new Config('http://localhost:15474'));

// MATCH-or-CREATE via Cypher (the legacy createNode REST endpoint is
// pending a routing fix — Cypher path works against the live server).
$ext = 'sha256:abc123';
$client->executeCypher(
    "CREATE (n:File {_id: '$ext', path: '/data/file.txt'}) ON CONFLICT MATCH RETURN n._id"
);

// Resolve by external id
$got = $client->getNodeByExternalId($ext);
if ($got['node'] !== null) {
    echo "found id={$got['node']['id']}\n";
}
```

Pulled verbatim from `sdks/php/tests/ExternalIdLiveTest.php` (14/14 live).

## Best Practices

1. **Choose a stable identifier**: Use content hash (SHA-256/BLAKE3) for files, UUID for logical entities, string keys for natural identifiers.

2. **Use MATCH for batch imports**: Re-running an import should be safe. `ON CONFLICT MATCH` makes it idempotent.

3. **Project `_id` in results**: When you need to round-trip identifiers back to the calling system, include `RETURN n._id` in queries.

4. **Avoid schema conflicts**: If your data already has a property named `_id`, rename it during import (or use a different external-id property name in a future version).

5. **Verify imports with count**: After import, query `MATCH (n:Label {_id: $id}) RETURN COUNT(n)` to verify a single node exists per external id.

6. **Document external id semantics**: In your schema documentation, note which node types use external ids and what identifier scheme (hash type, string format, etc.) they use.

## Troubleshooting

### Duplicate nodes created on re-import

**Problem**: Re-running an import creates new nodes instead of matching existing ones.

**Solution**: Use `ON CONFLICT MATCH` in your CREATE statements:
```cypher
CREATE (f:File {_id: 'sha256:…', path: '/file'}) ON CONFLICT MATCH
```

### External ID lookup returns null

**Problem**: `MATCH (n {_id: 'sha256:…'})` returns no rows.

**Solution**: Verify the external id format. Use `MATCH (n:Label) RETURN n._id` to see all external ids:
```cypher
MATCH (f:File) WHERE f._id IS NOT NULL RETURN f._id LIMIT 10
```

### Cannot query by external id (slow scan)

**Problem**: `MATCH (n {_id: 'sha256:…'})` scans all nodes instead of using the index.

**Solution**: This is expected only if you see a `LabelScan` instead of `ExternalIdSeek` in `EXPLAIN`. The planner should automatically choose the external-id index. If it doesn't, verify the external id is set correctly and check optimizer statistics:
```cypher
EXPLAIN MATCH (n {_id: 'sha256:…'}) RETURN n
```

## References

- **Specs**: `docs/specs/storage-format.md` (external-id catalog sub-databases)
- **Specs**: `docs/specs/cypher-subset.md` (reserved `_id` property)
- **Specs**: `docs/specs/api-protocols.md` (REST endpoints)
