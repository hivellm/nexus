## 1. LangChain Python integration
- [ ] 1.1 Scaffold `sdks/python/nexus_langchain/` package
- [ ] 1.2 Implement `NexusVectorStore` (LangChain `VectorStore` protocol — add_texts, similarity_search, similarity_search_with_score)
- [ ] 1.3 Implement `NexusGraphStore` (LangChain `GraphStore` protocol — query, add_graph_documents, refresh_schema)
- [ ] 1.4 Implement `NexusGraphRAGRetriever` (combines KNN + Cypher traversal in one call)
- [ ] 1.5 Add unit + integration tests against a local Nexus server

## 2. LlamaIndex Python integration
- [ ] 2.1 Scaffold `sdks/python/nexus_llamaindex/` package
- [ ] 2.2 Implement `NexusKnowledgeGraphIndex` (LlamaIndex KG protocol)
- [ ] 2.3 Implement `NexusVectorStoreIndex` (LlamaIndex VectorStore protocol)
- [ ] 2.4 Add unit + integration tests

## 3. LangChain.js integration
- [ ] 3.1 Scaffold `sdks/typescript/packages/nexus-langchain-js/` package
- [ ] 3.2 Implement `NexusVectorStore` for LangChain.js
- [ ] 3.3 Implement `NexusGraphStore` for LangChain.js
- [ ] 3.4 Add Vitest integration tests

## 4. Cookbook + examples
- [ ] 4.1 Create `docs/integrations/GRAPHRAG.md` end-to-end pipeline guide
- [ ] 4.2 Add `examples/graphrag/chat-over-docs/` (Python + TypeScript variants)
- [ ] 4.3 Add `examples/graphrag/recommendation/`
- [ ] 4.4 Add `examples/graphrag/knowledge-extraction/`
- [ ] 4.5 OpenAI, Anthropic, and Ollama backend examples in each

## 5. Distribution
- [ ] 5.1 Publish `nexus-langchain` to PyPI
- [ ] 5.2 Publish `nexus-llamaindex` to PyPI
- [ ] 5.3 Publish `@hivehub/nexus-langchain-js` to npm
- [ ] 5.4 Cross-link from README + main SDK READMEs

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 6.1 Update or create documentation covering the implementation
- [ ] 6.2 Write tests covering the new behavior
- [ ] 6.3 Run tests and confirm they pass
