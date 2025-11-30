# LangChain Nexus

LangChain integration for Nexus graph database with vector search and hybrid retrieval.

## Installation

```bash
pip install langchain-nexus
```

For OpenAI embeddings support:

```bash
pip install langchain-nexus[embeddings]
```

## Quick Start

### Vector Store

```python
from langchain_openai import OpenAIEmbeddings
from langchain_nexus import NexusVectorStore, NexusClient

# Create client
client = NexusClient("http://localhost:15474")

# Create vector store
embeddings = OpenAIEmbeddings()
vectorstore = NexusVectorStore(client, embeddings)

# Add documents
vectorstore.add_texts([
    "Machine learning is a subset of artificial intelligence.",
    "Deep learning uses neural networks with many layers.",
    "Natural language processing enables computers to understand text.",
])

# Search
docs = vectorstore.similarity_search("What is AI?", k=2)
for doc in docs:
    print(doc.page_content)
```

### Graph Retriever (Hybrid Search)

```python
from langchain_openai import OpenAIEmbeddings
from langchain_nexus import NexusGraphRetriever, NexusClient

client = NexusClient("http://localhost:15474")
embeddings = OpenAIEmbeddings()

# Create hybrid retriever (vector + graph)
retriever = NexusGraphRetriever(
    client=client,
    embedding=embeddings,
    hybrid_search=True,
    graph_depth=2,  # Traverse 2 hops
    k=4,  # Return top 4 results
)

# Retrieve documents
docs = retriever.invoke("What is machine learning?")
for doc in docs:
    print(f"Content: {doc.page_content}")
    print(f"Score: {doc.metadata.get('_rrf_score', 'N/A')}")
```

### Graph Memory

```python
from langchain_nexus import NexusGraphMemory, NexusClient

client = NexusClient("http://localhost:15474")

# Create conversation memory
memory = NexusGraphMemory(
    client=client,
    session_id="conversation-123",
    user_id="user-456",
)

# Add messages
memory.add_user_message("Hello, how are you?")
memory.add_ai_message("I'm doing great! How can I help you today?")

# Retrieve messages
for msg in memory.messages:
    print(f"{msg.type}: {msg.content}")

# Get conversation summary
summary = memory.get_conversation_summary()
print(f"Total messages: {summary['total_messages']}")
```

## Components

### NexusVectorStore

LangChain VectorStore implementation that stores documents as graph nodes with embeddings.

**Features:**
- Add documents with embeddings
- Similarity search with scores
- Search by vector
- Delete documents by ID
- Async support

**Parameters:**
- `client`: NexusClient instance
- `embedding`: Embeddings model (e.g., OpenAIEmbeddings)
- `label`: Node label for documents (default: "Document")
- `text_property`: Property name for text content (default: "text")
- `embedding_property`: Property name for embeddings (default: "embedding")

### NexusGraphRetriever

Hybrid retriever combining vector similarity search with graph traversal.

**Features:**
- Vector similarity search
- Graph-based context expansion
- Reciprocal Rank Fusion (RRF) for result merging
- Configurable graph depth
- Async support

**Parameters:**
- `client`: NexusClient instance
- `embedding`: Embeddings model
- `k`: Number of results to return (default: 4)
- `vector_k`: Number of vector search candidates (default: 10)
- `graph_depth`: Depth of graph traversal (default: 1)
- `rrf_k`: RRF constant for score fusion (default: 60)
- `hybrid_search`: Enable hybrid mode (default: True)

### NexusGraphMemory

Graph-based conversation memory for LangChain.

**Features:**
- Store messages as graph nodes
- Session and user tracking
- Message search
- Conversation summaries
- Async support

**Parameters:**
- `client`: NexusClient instance
- `session_id`: Unique session identifier
- `user_id`: Optional user identifier
- `window_size`: Number of messages to return (default: 10)

## Authentication

```python
# With API key
client = NexusClient(
    url="http://localhost:15474",
    api_key="your-api-key"
)

# With username/password
client = NexusClient(
    url="http://localhost:15474",
    username="admin",
    password="password"
)
```

## Async Usage

All components support async operations:

```python
import asyncio
from langchain_nexus import NexusVectorStore, NexusClient

async def main():
    client = NexusClient("http://localhost:15474")
    embeddings = OpenAIEmbeddings()
    vectorstore = NexusVectorStore(client, embeddings)

    # Async add
    await vectorstore.aadd_texts(["Hello world"])

    # Async search
    docs = await vectorstore.asimilarity_search("Hello")

    await client.close()

asyncio.run(main())
```

## Use with LangChain Chains

```python
from langchain_openai import ChatOpenAI
from langchain.chains import RetrievalQA
from langchain_nexus import NexusGraphRetriever, NexusClient

client = NexusClient("http://localhost:15474")
embeddings = OpenAIEmbeddings()
retriever = NexusGraphRetriever(client=client, embedding=embeddings)

llm = ChatOpenAI(model="gpt-4")
qa_chain = RetrievalQA.from_chain_type(
    llm=llm,
    chain_type="stuff",
    retriever=retriever,
)

answer = qa_chain.invoke("What is machine learning?")
print(answer)
```

## License

Apache 2.0
