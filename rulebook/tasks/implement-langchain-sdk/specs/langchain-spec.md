# LangChain Integration Specification for Nexus

## Vector Store Interface

### Python Implementation

**Class**: `NexusVectorStore`

**Base Class**: `langchain.vectorstores.base.VectorStore`

**Required Methods**:

```python
class NexusVectorStore(VectorStore):
    def __init__(
        self,
        nexus_url: str,
        api_key: Optional[str] = None,
        label: str = "Document",
        embedding_function: Optional[Embeddings] = None,
        **kwargs
    ):
        """Initialize Nexus vector store."""
    
    def add_texts(
        self,
        texts: List[str],
        metadatas: Optional[List[dict]] = None,
        ids: Optional[List[str]] = None,
        **kwargs
    ) -> List[str]:
        """Add texts to vector store."""
    
    def add_vectors(
        self,
        vectors: List[List[float]],
        texts: List[str],
        metadatas: Optional[List[dict]] = None,
        ids: Optional[List[str]] = None,
        **kwargs
    ) -> List[str]:
        """Add vectors to vector store."""
    
    def similarity_search(
        self,
        query: str,
        k: int = 4,
        filter: Optional[dict] = None,
        **kwargs
    ) -> List[Document]:
        """Search for similar documents."""
    
    def similarity_search_with_score(
        self,
        query: str,
        k: int = 4,
        filter: Optional[dict] = None,
        **kwargs
    ) -> List[Tuple[Document, float]]:
        """Search with similarity scores."""
    
    def delete(self, ids: Optional[List[str]] = None, **kwargs) -> bool:
        """Delete documents from vector store."""
    
    def as_retriever(self, **kwargs) -> NexusGraphRetriever:
        """Create retriever from vector store."""
```

### TypeScript Implementation

**Class**: `NexusVectorStore`

**Base Class**: `@langchain/core/vectorstores.VectorStore`

**Required Methods**:

```typescript
class NexusVectorStore extends VectorStore {
  constructor(
    nexusUrl: string,
    options?: {
      apiKey?: string;
      label?: string;
      embedding?: Embeddings;
    }
  );

  async addDocuments(
    documents: Document[],
    options?: { ids?: string[] }
  ): Promise<string[]>;

  async addVectors(
    vectors: number[][],
    documents: Document[],
    options?: { ids?: string[] }
  ): Promise<string[]>;

  async similaritySearch(
    query: string,
    k?: number,
    filter?: Record<string, any>
  ): Promise<Document[]>;

  async similaritySearchWithScore(
    query: string,
    k?: number,
    filter?: Record<string, any>
  ): Promise<[Document, number][]>;

  async delete(params: { ids?: string[] }): Promise<void>;

  asRetriever(kwargs?: any): NexusGraphRetriever;
}
```

## Memory Interface

### Python Implementation

**Class**: `NexusGraphMemory`

**Base Class**: `langchain.memory.base.BaseMemory`

**Required Methods**:

```python
class NexusGraphMemory(BaseMemory):
    def __init__(
        self,
        nexus_url: str,
        api_key: Optional[str] = None,
        memory_key: str = "history",
        return_messages: bool = False,
        **kwargs
    ):
        """Initialize graph memory."""
    
    @property
    def memory_variables(self) -> List[str]:
        """Return list of memory variable names."""
    
    def load_memory_variables(self, inputs: Dict[str, Any]) -> Dict[str, Any]:
        """Load memory variables."""
    
    def save_context(self, inputs: Dict[str, Any], outputs: Dict[str, str]) -> None:
        """Save conversation context to graph."""
    
    def clear(self) -> None:
        """Clear memory."""
```

### TypeScript Implementation

**Class**: `NexusGraphMemory`

**Base Class**: `@langchain/core/memory.BaseMemory`

**Required Methods**:

```typescript
class NexusGraphMemory extends BaseMemory {
  constructor(
    nexusUrl: string,
    options?: {
      apiKey?: string;
      memoryKey?: string;
      returnMessages?: boolean;
    }
  );

  get memoryKeys(): string[];

  async loadMemoryVariables(
    inputs: Record<string, any>
  ): Promise<Record<string, any>>;

  async saveContext(
    inputs: Record<string, any>,
    outputs: Record<string, string>
  ): Promise<void>;

  async clear(): Promise<void>;
}
```

## Retriever Interface

### Python Implementation

**Class**: `NexusGraphRetriever`

**Base Class**: `langchain.schema.BaseRetriever`

**Required Methods**:

```python
class NexusGraphRetriever(BaseRetriever):
    def __init__(
        self,
        nexus_url: str,
        api_key: Optional[str] = None,
        label: str = "Document",
        k: int = 4,
        use_hybrid_search: bool = True,
        traversal_depth: int = 1,
        **kwargs
    ):
        """Initialize graph retriever."""
    
    def _get_relevant_documents(
        self,
        query: str,
        *,
        run_manager: Optional[CallbackManagerForRetrieverRun] = None,
    ) -> List[Document]:
        """Get relevant documents."""
    
    async def _aget_relevant_documents(
        self,
        query: str,
        *,
        run_manager: Optional[AsyncCallbackManagerForRetrieverRun] = None,
    ) -> List[Document]:
        """Async get relevant documents."""
```

### TypeScript Implementation

**Class**: `NexusGraphRetriever`

**Base Class**: `@langchain/core/retrievers.BaseRetriever`

**Required Methods**:

```typescript
class NexusGraphRetriever extends BaseRetriever {
  constructor(
    nexusUrl: string,
    options?: {
      apiKey?: string;
      label?: string;
      k?: number;
      useHybridSearch?: boolean;
      traversalDepth?: number;
    }
  );

  async _getRelevantDocuments(
    query: string,
    runManager?: CallbackManager
  ): Promise<Document[]>;
}
```

## Hybrid Search Specification

### RRF (Reciprocal Rank Fusion)

**Algorithm**:
```
RRF(d) = Σ(1 / (k + rank_i(d)))
```

Where:
- `d` is a document
- `k` is the RRF constant (default: 60)
- `rank_i(d)` is the rank of document `d` in result set `i`

**Implementation**:

1. Execute vector similarity search
2. Execute graph traversal search
3. Combine results using RRF
4. Sort by RRF score
5. Return top-k results

### Graph Traversal Integration

**Features**:
- Expand results via relationships
- Follow citation graphs
- Traverse knowledge graphs
- Multi-hop reasoning
- Configurable traversal depth

## Document Graph Builder Specification

### Entity Extraction

**Process**:
1. Extract named entities from documents
2. Create nodes for each entity
3. Link entities to source documents
4. Extract relationships between entities

### Relationship Extraction

**Process**:
1. Identify entity co-occurrences
2. Extract semantic relationships
3. Create relationship edges
4. Store relationship metadata

### Graph Construction

**Structure**:
- Document nodes with embeddings
- Entity nodes
- Relationship edges
- Metadata as properties

## Authentication

### API Key Authentication

```python
vector_store = NexusVectorStore(
    nexus_url="http://localhost:15474",
    api_key="nexus_sk_...",
    label="Document"
)
```

### User/Password Authentication

```python
vector_store = NexusVectorStore(
    nexus_url="http://localhost:15474",
    username="user",
    password="pass",
    label="Document"
)
```

## Error Handling

### Error Types

1. **ConnectionError**: Failed to connect to Nexus
2. **AuthenticationError**: Invalid credentials
3. **QueryError**: Cypher query execution error
4. **ValidationError**: Invalid input parameters
5. **EmbeddingError**: Embedding generation error

### Error Response

```python
class NexusError(Exception):
    """Base exception for Nexus operations."""

class NexusConnectionError(NexusError):
    """Connection error."""

class NexusAuthenticationError(NexusError):
    """Authentication error."""

class NexusQueryError(NexusError):
    """Query execution error."""
```

## Testing Requirements

### Unit Tests

- Test all vector store methods
- Test memory operations
- Test retriever functionality
- Test hybrid search
- Test error handling
- ≥90% code coverage

### Integration Tests

- Test with real Nexus server
- Test with LangChain chains
- Test with LLM models
- Test end-to-end RAG workflows
- Test conversational agents

### Performance Tests

- Test search latency
- Test batch operations
- Test concurrent access
- Test memory usage

