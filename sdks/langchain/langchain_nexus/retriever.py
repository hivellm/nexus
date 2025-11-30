"""Nexus Graph Retriever implementation for LangChain."""

from __future__ import annotations

from typing import Any, Dict, List, Optional

from langchain_core.callbacks import CallbackManagerForRetrieverRun
from langchain_core.documents import Document
from langchain_core.embeddings import Embeddings
from langchain_core.retrievers import BaseRetriever
from pydantic import Field

from langchain_nexus.client import NexusClient


class NexusGraphRetriever(BaseRetriever):
    """LangChain Retriever with hybrid vector + graph search for Nexus.

    This retriever combines vector similarity search with graph traversal
    to provide context-aware document retrieval. It uses Reciprocal Rank
    Fusion (RRF) to merge results from both search methods.

    Args:
        client: NexusClient instance for database connection
        embedding: Embeddings model for generating vectors
        label: Node label for document nodes (default: "Document")
        k: Number of results to return (default: 4)
        vector_k: Number of vector search results (default: 10)
        graph_depth: Depth of graph traversal (default: 1)
        rrf_k: RRF constant for score fusion (default: 60)
        hybrid_search: Enable hybrid vector + graph search (default: True)
        text_property: Property name for document text (default: "text")
        embedding_property: Property name for embedding vector (default: "embedding")

    Example:
        >>> from langchain_openai import OpenAIEmbeddings
        >>> from langchain_nexus import NexusGraphRetriever, NexusClient
        >>>
        >>> client = NexusClient("http://localhost:15474")
        >>> embeddings = OpenAIEmbeddings()
        >>> retriever = NexusGraphRetriever(
        ...     client=client,
        ...     embedding=embeddings,
        ...     hybrid_search=True,
        ...     graph_depth=2,
        ... )
        >>>
        >>> docs = retriever.invoke("What is machine learning?")
    """

    client: NexusClient = Field(description="Nexus database client")
    embedding: Embeddings = Field(description="Embeddings model")
    label: str = Field(default="Document", description="Node label for documents")
    k: int = Field(default=4, description="Number of results to return")
    vector_k: int = Field(default=10, description="Number of vector search results")
    graph_depth: int = Field(default=1, description="Depth of graph traversal")
    rrf_k: int = Field(default=60, description="RRF constant for score fusion")
    hybrid_search: bool = Field(default=True, description="Enable hybrid search")
    text_property: str = Field(default="text", description="Text property name")
    embedding_property: str = Field(default="embedding", description="Embedding property name")
    metadata_prefix: str = Field(default="metadata_", description="Metadata property prefix")

    class Config:
        arbitrary_types_allowed = True

    def _extract_metadata(self, properties: Dict[str, Any]) -> Dict[str, Any]:
        """Extract metadata from node properties."""
        metadata = {}
        prefix_len = len(self.metadata_prefix)

        for key, value in properties.items():
            if key.startswith(self.metadata_prefix):
                metadata[key[prefix_len:]] = value

        return metadata

    def _rrf_score(self, rank: int) -> float:
        """Calculate RRF score for a given rank.

        Args:
            rank: 1-based rank position

        Returns:
            RRF score
        """
        return 1.0 / (self.rrf_k + rank)

    def _merge_results(
        self,
        vector_results: List[Dict[str, Any]],
        graph_results: List[Dict[str, Any]],
    ) -> List[Dict[str, Any]]:
        """Merge vector and graph results using RRF.

        Args:
            vector_results: Results from vector search
            graph_results: Results from graph traversal

        Returns:
            Merged and re-ranked results
        """
        # Build score maps
        doc_scores: Dict[str, float] = {}
        doc_data: Dict[str, Dict[str, Any]] = {}

        # Add vector search scores
        for rank, result in enumerate(vector_results, 1):
            doc_id = str(result.get("node_id", result.get("id", rank)))
            doc_scores[doc_id] = doc_scores.get(doc_id, 0) + self._rrf_score(rank)
            doc_data[doc_id] = result

        # Add graph traversal scores
        for rank, result in enumerate(graph_results, 1):
            doc_id = str(result.get("node_id", result.get("id", rank)))
            doc_scores[doc_id] = doc_scores.get(doc_id, 0) + self._rrf_score(rank)
            if doc_id not in doc_data:
                doc_data[doc_id] = result

        # Sort by combined score
        sorted_docs = sorted(doc_scores.items(), key=lambda x: x[1], reverse=True)

        # Return merged results
        merged = []
        for doc_id, score in sorted_docs[: self.k]:
            result = doc_data[doc_id].copy()
            result["rrf_score"] = score
            merged.append(result)

        return merged

    def _get_relevant_documents(
        self,
        query: str,
        *,
        run_manager: Optional[CallbackManagerForRetrieverRun] = None,
    ) -> List[Document]:
        """Retrieve relevant documents for a query.

        Args:
            query: Query string
            run_manager: Callback manager for run events

        Returns:
            List of relevant documents
        """
        # Generate query embedding
        query_embedding = self.embedding.embed_query(query)

        # Vector search
        vector_results = self.client.knn_search_sync(
            label=self.label,
            vector=query_embedding,
            k=self.vector_k,
            property_name=self.embedding_property,
        )

        if not self.hybrid_search or not vector_results:
            # Return vector results only
            return self._results_to_documents(vector_results)

        # Get node IDs from vector results for graph traversal
        node_ids = [
            result.get("node_id", result.get("id"))
            for result in vector_results
            if result.get("node_id") or result.get("id")
        ]

        if not node_ids:
            return self._results_to_documents(vector_results)

        # Graph traversal from vector search results
        graph_results = self._traverse_graph(node_ids)

        # Merge results using RRF
        merged_results = self._merge_results(vector_results, graph_results)

        return self._results_to_documents(merged_results)

    def _traverse_graph(self, node_ids: List[int]) -> List[Dict[str, Any]]:
        """Traverse graph from given node IDs.

        Args:
            node_ids: Starting node IDs

        Returns:
            List of connected nodes
        """
        if not node_ids:
            return []

        # Build Cypher query for graph traversal
        ids_str = ", ".join(str(nid) for nid in node_ids[:5])  # Limit starting nodes

        query = f"""
        MATCH (start:{self.label})
        WHERE id(start) IN [{ids_str}]
        MATCH (start)-[*1..{self.graph_depth}]-(related:{self.label})
        WHERE id(related) <> id(start)
        RETURN DISTINCT id(related) as node_id, related as node
        LIMIT {self.vector_k}
        """

        try:
            result = self.client.execute_cypher_sync(query)
            rows = result.get("rows", [])

            graph_results = []
            for row in rows:
                if len(row) >= 2:
                    node_id = row[0]
                    node_data = row[1] if isinstance(row[1], dict) else {}
                    graph_results.append({
                        "node_id": node_id,
                        "properties": node_data.get("properties", node_data),
                    })

            return graph_results
        except Exception:
            # If graph traversal fails, return empty list
            return []

    def _results_to_documents(
        self,
        results: List[Dict[str, Any]],
    ) -> List[Document]:
        """Convert search results to Document objects.

        Args:
            results: Search results from Nexus

        Returns:
            List of Document objects
        """
        documents = []

        for result in results[: self.k]:
            properties = result.get("properties", result)
            text = properties.get(self.text_property, "")
            metadata = self._extract_metadata(properties)

            # Add search metadata
            metadata["_node_id"] = result.get("node_id", result.get("id"))
            if "score" in result:
                metadata["_score"] = result["score"]
            if "rrf_score" in result:
                metadata["_rrf_score"] = result["rrf_score"]
            if "similarity" in result:
                metadata["_similarity"] = result["similarity"]

            doc = Document(page_content=text, metadata=metadata)
            documents.append(doc)

        return documents

    async def _aget_relevant_documents(
        self,
        query: str,
        *,
        run_manager: Optional[CallbackManagerForRetrieverRun] = None,
    ) -> List[Document]:
        """Retrieve relevant documents for a query asynchronously.

        Args:
            query: Query string
            run_manager: Callback manager for run events

        Returns:
            List of relevant documents
        """
        # Generate query embedding
        query_embedding = await self.embedding.aembed_query(query)

        # Vector search
        vector_results = await self.client.knn_search(
            label=self.label,
            vector=query_embedding,
            k=self.vector_k,
            property_name=self.embedding_property,
        )

        if not self.hybrid_search or not vector_results:
            return self._results_to_documents(vector_results)

        # Get node IDs from vector results
        node_ids = [
            result.get("node_id", result.get("id"))
            for result in vector_results
            if result.get("node_id") or result.get("id")
        ]

        if not node_ids:
            return self._results_to_documents(vector_results)

        # Graph traversal
        graph_results = await self._atraverse_graph(node_ids)

        # Merge results using RRF
        merged_results = self._merge_results(vector_results, graph_results)

        return self._results_to_documents(merged_results)

    async def _atraverse_graph(self, node_ids: List[int]) -> List[Dict[str, Any]]:
        """Traverse graph from given node IDs asynchronously.

        Args:
            node_ids: Starting node IDs

        Returns:
            List of connected nodes
        """
        if not node_ids:
            return []

        ids_str = ", ".join(str(nid) for nid in node_ids[:5])

        query = f"""
        MATCH (start:{self.label})
        WHERE id(start) IN [{ids_str}]
        MATCH (start)-[*1..{self.graph_depth}]-(related:{self.label})
        WHERE id(related) <> id(start)
        RETURN DISTINCT id(related) as node_id, related as node
        LIMIT {self.vector_k}
        """

        try:
            result = await self.client.execute_cypher(query)
            rows = result.get("rows", [])

            graph_results = []
            for row in rows:
                if len(row) >= 2:
                    node_id = row[0]
                    node_data = row[1] if isinstance(row[1], dict) else {}
                    graph_results.append({
                        "node_id": node_id,
                        "properties": node_data.get("properties", node_data),
                    })

            return graph_results
        except Exception:
            return []
