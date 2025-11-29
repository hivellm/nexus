"""LangChain integration for Nexus graph database.

This package provides LangChain-compatible components for interacting with
Nexus graph database, including:

- NexusVectorStore: Vector store with graph-enhanced retrieval
- NexusGraphRetriever: Hybrid retriever combining vector search and graph traversal
- NexusGraphMemory: Graph-based conversation memory
- NexusDocumentGraphBuilder: Build knowledge graphs from documents
"""

from langchain_nexus.vectorstore import NexusVectorStore
from langchain_nexus.retriever import NexusGraphRetriever
from langchain_nexus.memory import NexusGraphMemory
from langchain_nexus.client import NexusClient

__all__ = [
    "NexusVectorStore",
    "NexusGraphRetriever",
    "NexusGraphMemory",
    "NexusClient",
]

__version__ = "0.1.0"
