# LangFlow Component Specification for Nexus

## Component Structure

### Base Component Class

All Nexus components MUST extend LangFlow's base component class:

```python
from langflow.custom import Component
from langflow.inputs import StrInput, IntInput, BoolInput
from langflow.outputs import DataOutput

class NexusBaseComponent(Component):
    """Base class for all Nexus components."""
    
    display_name: str = "Nexus Component"
    description: str = "Nexus graph database component"
    icon: str = "graph"
    category: str = "Database"
    
    def build_config(self):
        """Define component configuration fields."""
        return {
            "nexus_url": StrInput(
                display_name="Nexus URL",
                value="http://localhost:15474",
                required=True
            ),
            "api_key": StrInput(
                display_name="API Key",
                password=True,
                required=False
            )
        }
```

## Component Specifications

### Nexus Vector Store Component

**Class**: `NexusVectorStoreComponent`

**Display Name**: "Nexus Vector Store"

**Description**: "Store documents with embeddings in Nexus graph database"

**Inputs**:
- `documents` (DataInput) - Documents to store
- `embeddings` (DataInput, optional) - Pre-computed embeddings
- `metadata` (DataInput, optional) - Document metadata

**Outputs**:
- `document_ids` (DataOutput) - IDs of stored documents
- `status` (DataOutput) - Operation status

**Configuration**:
- `nexus_url` (StrInput) - Nexus server URL
- `api_key` (StrInput, password) - API key credential
- `label` (StrInput) - Node label for documents (default: "Document")
- `embedding_model` (DropdownInput) - Embedding model selection
- `batch_size` (IntInput) - Batch size for ingestion (default: 100)

**Implementation**:
```python
class NexusVectorStoreComponent(NexusBaseComponent):
    def build(self, documents, embeddings=None, metadata=None):
        # Initialize vector store
        vector_store = NexusVectorStore(
            nexus_url=self.nexus_url,
            api_key=self.api_key,
            label=self.label
        )
        
        # Add documents
        ids = vector_store.add_texts(
            texts=documents,
            metadatas=metadata,
            embeddings=embeddings
        )
        
        return {"document_ids": ids, "status": "success"}
```

### Nexus Graph Retriever Component

**Class**: `NexusGraphRetrieverComponent`

**Display Name**: "Nexus Graph Retriever"

**Description**: "Retrieve documents using hybrid search (vector + graph)"

**Inputs**:
- `query` (StrInput) - Search query
- `context` (DataInput, optional) - Additional context

**Outputs**:
- `documents` (DataOutput) - Retrieved documents
- `scores` (DataOutput) - Similarity scores
- `graph_context` (DataOutput) - Graph traversal context

**Configuration**:
- `nexus_url` (StrInput) - Nexus server URL
- `api_key` (StrInput, password) - API key credential
- `label` (StrInput) - Label to search (default: "Document")
- `k` (IntInput) - Number of results (default: 4)
- `use_hybrid_search` (BoolInput) - Enable hybrid search (default: True)
- `traversal_depth` (IntInput) - Graph traversal depth (default: 1)
- `rrf_constant` (IntInput) - RRF constant k (default: 60)

**Implementation**:
```python
class NexusGraphRetrieverComponent(NexusBaseComponent):
    def build(self, query, context=None):
        # Initialize retriever
        retriever = NexusGraphRetriever(
            nexus_url=self.nexus_url,
            api_key=self.api_key,
            label=self.label,
            k=self.k,
            use_hybrid_search=self.use_hybrid_search,
            traversal_depth=self.traversal_depth
        )
        
        # Retrieve documents
        documents = retriever.get_relevant_documents(query)
        
        return {
            "documents": documents,
            "scores": [doc.metadata.get("score") for doc in documents],
            "graph_context": documents[0].metadata.get("graph_context", [])
        }
```

### Nexus Graph Memory Component

**Class**: `NexusGraphMemoryComponent`

**Display Name**: "Nexus Graph Memory"

**Description**: "Store conversation context in Nexus graph"

**Inputs**:
- `messages` (DataInput) - Conversation messages
- `user_input` (StrInput) - User input
- `ai_response` (StrInput) - AI response

**Outputs**:
- `memory_variables` (DataOutput) - Loaded memory variables
- `conversation_history` (DataOutput) - Full conversation history
- `related_conversations` (DataOutput) - Related conversations from graph

**Configuration**:
- `nexus_url` (StrInput) - Nexus server URL
- `api_key` (StrInput, password) - API key credential
- `memory_key` (StrInput) - Memory key name (default: "history")
- `return_messages` (BoolInput) - Return as messages (default: False)
- `enable_graph_traversal` (BoolInput) - Enable graph traversal (default: True)
- `max_related` (IntInput) - Max related conversations (default: 5)

**Implementation**:
```python
class NexusGraphMemoryComponent(NexusBaseComponent):
    def build(self, messages, user_input, ai_response):
        # Initialize memory
        memory = NexusGraphMemory(
            nexus_url=self.nexus_url,
            api_key=self.api_key,
            memory_key=self.memory_key,
            return_messages=self.return_messages
        )
        
        # Save context
        memory.save_context(
            {"input": user_input},
            {"output": ai_response}
        )
        
        # Load memory
        memory_vars = memory.load_memory_variables({})
        
        return {
            "memory_variables": memory_vars,
            "conversation_history": memory_vars.get(self.memory_key, []),
            "related_conversations": memory.get_related_conversations()
        }
```

### Nexus Document Graph Builder Component

**Class**: `NexusDocumentGraphBuilderComponent`

**Display Name**: "Nexus Document Graph Builder"

**Description**: "Build knowledge graph from documents"

**Inputs**:
- `documents` (DataInput) - Documents to process
- `entity_model` (DataInput, optional) - Entity extraction model
- `relationship_model` (DataInput, optional) - Relationship extraction model

**Outputs**:
- `graph_stats` (DataOutput) - Graph statistics
- `entities` (DataOutput) - Extracted entities
- `relationships` (DataOutput) - Extracted relationships
- `visualization_data` (DataOutput) - Graph visualization data

**Configuration**:
- `nexus_url` (StrInput) - Nexus server URL
- `api_key` (StrInput, password) - API key credential
- `enable_entity_extraction` (BoolInput) - Extract entities (default: True)
- `enable_relationship_extraction` (BoolInput) - Extract relationships (default: True)
- `entity_types` (ListInput) - Entity types to extract
- `relationship_types` (ListInput) - Relationship types to extract

**Implementation**:
```python
class NexusDocumentGraphBuilderComponent(NexusBaseComponent):
    def build(self, documents, entity_model=None, relationship_model=None):
        # Initialize builder
        builder = NexusDocumentGraphBuilder(
            nexus_url=self.nexus_url,
            api_key=self.api_key
        )
        
        # Build graph
        result = builder.build_graph(
            documents=documents,
            extract_entities=self.enable_entity_extraction,
            extract_relationships=self.enable_relationship_extraction
        )
        
        return {
            "graph_stats": result.stats,
            "entities": result.entities,
            "relationships": result.relationships,
            "visualization_data": result.to_visualization_format()
        }
```

### Nexus Hybrid Search Component

**Class**: `NexusHybridSearchComponent`

**Display Name**: "Nexus Hybrid Search"

**Description**: "Combine vector search and graph traversal with RRF ranking"

**Inputs**:
- `query` (StrInput) - Search query
- `starting_nodes` (DataInput, optional) - Graph traversal starting points

**Outputs**:
- `results` (DataOutput) - Combined search results
- `vector_results` (DataOutput) - Vector search results
- `graph_results` (DataOutput) - Graph traversal results
- `rrf_scores` (DataOutput) - RRF ranking scores

**Configuration**:
- `nexus_url` (StrInput) - Nexus server URL
- `api_key` (StrInput, password) - API key credential
- `label` (StrInput) - Label to search (default: "Document")
- `k` (IntInput) - Number of results (default: 10)
- `vector_weight` (FloatInput) - Vector search weight (default: 0.5)
- `graph_weight` (FloatInput) - Graph search weight (default: 0.5)
- `rrf_constant` (IntInput) - RRF constant k (default: 60)
- `traversal_depth` (IntInput) - Graph traversal depth (default: 2)

**Implementation**:
```python
class NexusHybridSearchComponent(NexusBaseComponent):
    def build(self, query, starting_nodes=None):
        # Initialize hybrid search
        searcher = NexusHybridSearch(
            nexus_url=self.nexus_url,
            api_key=self.api_key,
            label=self.label,
            rrf_constant=self.rrf_constant
        )
        
        # Execute hybrid search
        results = searcher.search(
            query=query,
            k=self.k,
            starting_nodes=starting_nodes,
            traversal_depth=self.traversal_depth
        )
        
        return {
            "results": results.combined,
            "vector_results": results.vector,
            "graph_results": results.graph,
            "rrf_scores": results.rrf_scores
        }
```

## UI Configuration

### Form Fields

All components MUST provide:

1. **Connection Fields**
   - Nexus URL (text input with validation)
   - API Key (password input with credential management)
   - Connection test button

2. **Operation-Specific Fields**
   - Relevant to component operation
   - With default values
   - With validation
   - With help tooltips

3. **Advanced Options**
   - Collapsible section
   - Optional parameters
   - Performance tuning options

### Validation

- URL format validation
- Required field validation
- Type validation
- Range validation for numeric fields
- Connection validation before execution

### Error Display

- Clear error messages
- Error location indication
- Recovery suggestions
- Link to documentation

## Result Visualization

### Document Results

- Table view
- List view
- Card view
- Graph view (for graph results)

### Statistics

- Operation metrics
- Performance metrics
- Graph statistics
- Search statistics

### Graph Visualization

- Node-link diagram
- Interactive exploration
- Filtering options
- Export options

## Testing Requirements

### Unit Tests

- Test component classes
- Test configuration handling
- Test input/output processing
- Test error handling
- ≥90% code coverage

### Integration Tests

- Test with LangFlow
- Test component loading
- Test workflow execution
- Test template workflows

### UI Tests

- Test configuration forms
- Test result display
- Test error messages
- Test user interactions

