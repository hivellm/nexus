# LangFlow Nexus Components

Custom LangFlow components for Nexus graph database integration.

## Installation

```bash
pip install langflow-nexus
```

## Components

### Nexus Connection

Creates a connection to a Nexus graph database server.

**Inputs:**
- **Server URL**: Nexus server URL (e.g., http://localhost:15474)
- **API Key**: Optional API key for authentication
- **Username/Password**: Optional basic authentication
- **Timeout**: Request timeout in seconds

**Outputs:**
- **Client**: NexusClient instance for use with other components
- **Connection Status**: Connection health check result

### Nexus Vector Store

Store and search documents using vector embeddings.

**Inputs:**
- **Nexus Client**: From NexusConnection component
- **Embedding Model**: Any LangFlow embedding component
- **Node Label**: Label for document nodes (default: "Document")
- **Documents**: Optional documents to add
- **Number of Results**: K for similarity search

**Outputs:**
- **Vector Store**: NexusVectorStore instance
- **Retriever**: Retriever for use in chains

### Nexus Vector Search

Search for similar documents in the vector store.

**Inputs:**
- **Vector Store**: NexusVectorStore instance
- **Search Query**: Query text
- **Number of Results**: K results to return

**Outputs:**
- **Documents**: List of matching documents
- **Results with Scores**: Documents with similarity scores

### Nexus Graph Retriever

Hybrid retriever combining vector search with graph traversal.

**Inputs:**
- **Nexus Client**: From NexusConnection component
- **Embedding Model**: Any LangFlow embedding component
- **Node Label**: Label for document nodes
- **Number of Results**: Final results to return
- **Vector Search Candidates**: Initial vector search count
- **Graph Traversal Depth**: Hops to traverse in graph
- **Hybrid Search**: Enable/disable hybrid mode
- **RRF Constant**: Reciprocal Rank Fusion constant

**Outputs:**
- **Retriever**: NexusGraphRetriever for use in chains

### Nexus Hybrid Search

Perform hybrid vector + graph search.

**Inputs:**
- **Graph Retriever**: NexusGraphRetriever instance
- **Search Query**: Query text

**Outputs:**
- **Documents**: Retrieved documents
- **Results with Scores**: Documents with RRF scores

### Nexus Graph Traversal

Traverse graph relationships from a starting node.

**Inputs:**
- **Nexus Client**: From NexusConnection component
- **Start Node ID**: ID of starting node
- **Relationship Types**: Comma-separated types (optional)
- **Max Depth**: Maximum traversal depth
- **Target Label**: Optional label filter

**Outputs:**
- **Connected Nodes**: Nodes found via traversal

### Nexus Graph Memory

Graph-based conversation memory.

**Inputs:**
- **Nexus Client**: From NexusConnection component
- **Session ID**: Unique session identifier
- **User ID**: Optional user identifier
- **Window Size**: Recent messages to retrieve

**Outputs:**
- **Memory**: NexusGraphMemory instance
- **Messages**: Recent conversation messages
- **Summary**: Conversation statistics

### Nexus Add Message

Add messages to conversation memory.

**Inputs:**
- **Memory**: NexusGraphMemory instance
- **Message**: Message content
- **Message Type**: human, ai, or system

**Outputs:**
- **Status**: Operation result

### Nexus Search Messages

Search conversation history.

**Inputs:**
- **Memory**: NexusGraphMemory instance
- **Keyword**: Search keyword
- **Limit**: Maximum results

**Outputs:**
- **Messages**: Matching messages

### Nexus Clear Memory

Clear conversation history.

**Inputs:**
- **Memory**: NexusGraphMemory instance

**Outputs:**
- **Status**: Operation result

## Usage Examples

### Basic RAG Pipeline

1. Add **Nexus Connection** component
2. Add **OpenAI Embeddings** component
3. Add **Nexus Vector Store** component
   - Connect client from Nexus Connection
   - Connect embeddings from OpenAI Embeddings
4. Add **Nexus Vector Search** component
   - Connect vector store
   - Enter your query
5. Connect to **Chat** component for responses

### Hybrid Search Pipeline

1. Add **Nexus Connection** component
2. Add **OpenAI Embeddings** component
3. Add **Nexus Graph Retriever** component
   - Connect client and embeddings
   - Set graph_depth to 2
   - Enable hybrid_search
4. Add **Nexus Hybrid Search** component
   - Connect retriever
   - Enter your query
5. Use results in your chain

### Conversation Memory

1. Add **Nexus Connection** component
2. Add **Nexus Graph Memory** component
   - Connect client
   - Set session_id (e.g., use a UUID)
3. Add **Nexus Add Message** component
   - Connect memory
   - Use for storing chat history
4. Use memory output with LLM chains

## Configuration

### Environment Variables

You can set default configuration via environment variables:

```bash
export NEXUS_URL=http://localhost:15474
export NEXUS_API_KEY=your-api-key
```

### Custom Labels

All components support custom node labels:

- **Node Label**: For document nodes
- **Message Label**: For chat message nodes
- **Session Label**: For conversation session nodes

## License

Apache 2.0
