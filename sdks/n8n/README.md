# n8n-nodes-nexus

This is an n8n community node that provides integration with **Nexus Graph Database**. It allows you to execute Cypher queries and perform graph operations directly from your n8n workflows.

[n8n](https://n8n.io/) is a fair-code licensed workflow automation platform.

[Nexus](https://github.com/hivellm/nexus) is a high-performance graph database with Neo4j-compatible Cypher support.

## Installation

Follow the [installation guide](https://docs.n8n.io/integrations/community-nodes/installation/) in the n8n community nodes documentation.

```bash
npm install @hivellm/n8n-nodes-nexus
```

Or install via n8n UI:
1. Go to **Settings** > **Community Nodes**
2. Select **Install a community node**
3. Enter `@hivellm/n8n-nodes-nexus`
4. Click **Install**

## Operations

The Nexus node supports the following operations:

### Query Operations
- **Execute Cypher**: Execute any Cypher query with parameter support

### Node Operations
- **Create Node**: Create a new node with labels and properties
- **Read Node**: Read a node by ID
- **Update Node**: Update node properties
- **Delete Node**: Delete a node (with optional DETACH)
- **Find Nodes**: Find nodes by label and properties

### Relationship Operations
- **Create Relationship**: Create a relationship between two nodes
- **Read Relationship**: Read a relationship by ID
- **Update Relationship**: Update relationship properties
- **Delete Relationship**: Delete a relationship

### Batch Operations
- **Batch Create Nodes**: Create multiple nodes in a single operation
- **Batch Create Relationships**: Create multiple relationships in a single operation

### Schema Operations
- **List Labels**: Get all node labels in the database
- **List Relationship Types**: Get all relationship types
- **Get Schema**: Get complete schema information

### Graph Algorithms
- **Shortest Path**: Find the shortest path between two nodes

## Credentials

The node supports two authentication methods:

### API Key Authentication
- **Host**: Nexus server hostname (default: `localhost`)
- **Port**: Nexus server port (default: `15474`)
- **API Key**: Your Nexus API key
- **Use HTTPS**: Enable for secure connections

### User/Password Authentication
- **Host**: Nexus server hostname (default: `localhost`)
- **Port**: Nexus server port (default: `15474`)
- **Username**: Nexus username
- **Password**: Nexus password
- **Use HTTPS**: Enable for secure connections

## High Availability with Replication

Nexus supports master-replica replication for high availability and read scaling.
In n8n workflows, use **separate credentials** for master (writes) and replicas (reads).

### Setting Up Replication Credentials

1. **Create Master Credentials**:
   - Go to **Credentials** > **Add Credential**
   - Select **Nexus API** or **Nexus User**
   - Name: `Nexus Master`
   - Host: `master.yourdomain.com`
   - Port: `15474`

2. **Create Replica Credentials**:
   - Add another credential
   - Name: `Nexus Replica 1`
   - Host: `replica1.yourdomain.com`
   - Port: `15474`

3. **Create Second Replica** (optional):
   - Name: `Nexus Replica 2`
   - Host: `replica2.yourdomain.com`
   - Port: `15474`

### Workflow Pattern: Write to Master, Read from Replicas

```
┌─────────────────────────────────────────────────────────────┐
│                     n8n Workflow                             │
│                                                              │
│   ┌─────────────┐     ┌─────────────┐     ┌─────────────┐   │
│   │   Trigger   │────▶│   Nexus     │────▶│   Nexus     │   │
│   │   (HTTP)    │     │   (Write)   │     │   (Read)    │   │
│   └─────────────┘     │  ↓ Master   │     │  ↓ Replica  │   │
│                       └─────────────┘     └─────────────┘   │
└─────────────────────────────────────────────────────────────┘
                              │                    │
                              ▼                    ▼
              ┌───────────────────┐     ┌─────────────────────┐
              │      MASTER       │     │      REPLICAS       │
              │   (writes only)   │────▶│   (reads only)      │
              │                   │ WAL │  ┌───────────────┐  │
              │ • CREATE          │────▶│  │   Replica 1   │  │
              │ • UPDATE          │     │  └───────────────┘  │
              │ • DELETE          │     │  ┌───────────────┐  │
              │ • MERGE           │────▶│  │   Replica 2   │  │
              └───────────────────┘     │  └───────────────┘  │
                                        └─────────────────────┘
```

### Example: High Availability Data Sync Workflow

```
1. Schedule Trigger (every 5 minutes)
        │
        ▼
2. Nexus (Read) ──────────────────────────────▶ Replica credentials
   Query: MATCH (n:Person) WHERE n.synced = false RETURN n LIMIT 100
        │
        ▼
3. External API
   Process records
        │
        ▼
4. Nexus (Write) ─────────────────────────────▶ Master credentials
   Query: MATCH (n:Person) WHERE id(n) = $id SET n.synced = true
```

### Load Balancing Reads Across Replicas

For workflows with high read volume, use multiple Nexus nodes with different replica credentials:

```
                    ┌─────────────────┐
                    │     Switch      │
                    │  (Round Robin)  │
                    └────────┬────────┘
                             │
          ┌──────────────────┼──────────────────┐
          │                  │                  │
          ▼                  ▼                  ▼
    ┌───────────┐      ┌───────────┐      ┌───────────┐
    │  Nexus    │      │  Nexus    │      │  Nexus    │
    │ Replica 1 │      │ Replica 2 │      │  Master   │
    └───────────┘      └───────────┘      └───────────┘
    (30% traffic)      (30% traffic)      (40% traffic)
```

### Starting a Nexus Cluster

```bash
# Start master node
NEXUS_REPLICATION_ROLE=master \
NEXUS_REPLICATION_BIND_ADDR=0.0.0.0:15475 \
./nexus-server

# Start replica 1
NEXUS_REPLICATION_ROLE=replica \
NEXUS_REPLICATION_MASTER_ADDR=master:15475 \
./nexus-server

# Start replica 2
NEXUS_REPLICATION_ROLE=replica \
NEXUS_REPLICATION_MASTER_ADDR=master:15475 \
./nexus-server
```

### Monitoring Replication in n8n

Create a monitoring workflow to check replication health:

```
1. Schedule Trigger (every 1 minute)
        │
        ▼
2. HTTP Request
   GET http://master:15474/replication/status
        │
        ▼
3. IF node
   Check: status.running == true && status.replica_count > 0
        │
   ┌────┴────┐
   │         │
   ▼         ▼
 OK       ALERT
          Send notification
```

## Usage Examples

### Example 1: Execute Cypher Query

Query all Person nodes:

```cypher
MATCH (n:Person) RETURN n.name, n.age LIMIT 10
```

With parameters:
```cypher
MATCH (n:Person) WHERE n.age > $minAge RETURN n
```
Parameters: `minAge = 25`

### Example 2: Create a Node

1. Select **Create Node** operation
2. Set Labels: `Person`
3. Add Properties:
   - `name`: `Alice`
   - `age`: `30`
   - `city`: `New York`

### Example 3: Create a Relationship

1. Select **Create Relationship** operation
2. Set Start Node ID: `1`
3. Set End Node ID: `2`
4. Set Relationship Type: `KNOWS`
5. Add Properties:
   - `since`: `2024-01-01`

### Example 4: Batch Create Nodes

1. Select **Batch Create Nodes** operation
2. Enter Nodes JSON:
```json
[
  {"labels": ["Person"], "properties": {"name": "Bob", "age": 25}},
  {"labels": ["Person"], "properties": {"name": "Carol", "age": 28}},
  {"labels": ["Company"], "properties": {"name": "Acme Corp"}}
]
```

### Example 5: Find Shortest Path

1. Select **Shortest Path** operation
2. Set Start Node ID: `1`
3. Set End Node ID: `10`
4. Set Relationship Types Filter: `KNOWS,WORKS_WITH` (optional)
5. Set Max Depth: `5`

## Workflow Examples

### Data Import Workflow

Import data from a CSV file into the graph:

1. **Read CSV** node → reads data from file
2. **Nexus** node (Batch Create Nodes) → creates nodes from CSV rows
3. **Nexus** node (Batch Create Relationships) → creates relationships

### Graph Analysis Workflow

Analyze social network connections:

1. **HTTP Request** → trigger workflow
2. **Nexus** (Execute Cypher) → `MATCH (n:Person)-[:KNOWS]->(m) RETURN n.name, count(m)`
3. **Set** node → process results
4. **Send Email** → send report

### ETL Workflow

Transform relational data to graph:

1. **Postgres** → query source data
2. **Code** → transform to graph format
3. **Nexus** (Batch Create Nodes) → create nodes
4. **Nexus** (Batch Create Relationships) → create edges

## Error Handling

The node handles errors gracefully and provides detailed error messages:

- **Connection Error**: Failed to connect to Nexus server
- **Authentication Error**: Invalid credentials
- **Query Error**: Cypher syntax or execution error
- **Not Found Error**: Node or relationship not found

Enable **Continue on Fail** to handle errors without stopping the workflow.

## Resources

- [Nexus Documentation](https://github.com/hivellm/nexus)
- [Cypher Query Language](https://neo4j.com/docs/cypher-manual/current/)
- [n8n Community Nodes](https://docs.n8n.io/integrations/community-nodes/)

## License

[MIT](LICENSE)
