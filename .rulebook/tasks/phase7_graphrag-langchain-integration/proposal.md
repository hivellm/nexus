# Proposal: phase7_graphrag-langchain-integration

## Why

GraphRAG (graph-traversal + vector-search retrieval for LLMs) is the dominant 2025–2026 narrative axis in graph databases. Neo4j shipped Aura Agent (turnkey copilot). Memgraph launched a "GraphRAG toolkit for non-graph users" (Nov 2025). FalkorDB's entire product positioning is GraphRAG. ArangoDB markets "HybridGraphRAG" with FAISS + ArangoSearch. Nexus has all the engine-side pieces (per-label HNSW, Tantivy FTS, hybrid graph + vector queries, binary-RPC bytes-native embeddings) but ships **zero packaged integrations**. A user wanting to build a GraphRAG pipeline today writes the LangChain or LlamaIndex glue from scratch.

## What Changes

- Publish a Python sub-package `nexus-langchain` exposing a `NexusGraphStore`, `NexusVectorStore`, and `NexusGraphRAGRetriever` matching the LangChain `BaseRetriever` and `VectorStore` protocols.
- Publish a Python sub-package `nexus-llamaindex` with the equivalent `KnowledgeGraphIndex` + `VectorStoreIndex` integrations.
- Publish a TypeScript sub-package `@hivehub/nexus-langchain-js` matching LangChain.js.
- Cookbook in `docs/integrations/GRAPHRAG.md`: end-to-end pipeline (chunk text → embed → write to Nexus → retrieve via hybrid Cypher+KNN → feed to LLM). Use OpenAI / Anthropic / Ollama backends as examples.
- Reference apps under `examples/graphrag/`: chat-over-docs, recommendation, knowledge-extraction.

## Impact

- Affected specs: new `docs/integrations/GRAPHRAG.md`.
- Affected code: new `sdks/python/nexus_langchain/`, `sdks/python/nexus_llamaindex/`, `sdks/typescript/packages/nexus-langchain-js/`, `examples/graphrag/`.
- Breaking change: NO (additive).
- User benefit: closes the biggest competitive narrative gap; adds turnkey LLM-app onramp.
