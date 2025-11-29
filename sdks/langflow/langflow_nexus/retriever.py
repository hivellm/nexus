"""Nexus Graph Retriever Component for LangFlow."""

from typing import List

from langflow.custom import Component
from langflow.io import (
    BoolInput,
    HandleInput,
    IntInput,
    MessageTextInput,
    Output,
)
from langflow.schema import Data
from langchain_core.documents import Document

from langchain_nexus import NexusGraphRetriever, NexusClient


class NexusGraphRetrieverComponent(Component):
    """LangFlow component for Nexus Graph Retriever.

    This component provides hybrid retrieval combining vector
    similarity search with graph traversal for context-aware
    document retrieval.

    Inputs:
        client: NexusClient from NexusConnection component
        embedding: Embeddings model for query vectors
        label: Node label for documents
        k: Number of final results to return
        vector_k: Number of vector search candidates
        graph_depth: Depth of graph traversal
        hybrid_search: Enable hybrid vector + graph search
        rrf_k: RRF constant for score fusion

    Outputs:
        retriever: NexusGraphRetriever instance
    """

    display_name = "Nexus Graph Retriever"
    description = "Hybrid retriever combining vector search with graph traversal"
    icon = "git-branch"
    name = "NexusGraphRetriever"

    inputs = [
        HandleInput(
            name="client",
            display_name="Nexus Client",
            info="NexusClient from NexusConnection component",
            input_types=["NexusClient"],
            required=True,
        ),
        HandleInput(
            name="embedding",
            display_name="Embedding Model",
            info="Embeddings model for query vectors",
            input_types=["Embeddings"],
            required=True,
        ),
        MessageTextInput(
            name="label",
            display_name="Node Label",
            info="Label for document nodes",
            value="Document",
            required=False,
        ),
        IntInput(
            name="k",
            display_name="Number of Results",
            info="Number of final results to return",
            value=4,
            required=False,
        ),
        IntInput(
            name="vector_k",
            display_name="Vector Search Candidates",
            info="Number of vector search candidates",
            value=10,
            required=False,
        ),
        IntInput(
            name="graph_depth",
            display_name="Graph Traversal Depth",
            info="How many hops to traverse in the graph",
            value=1,
            required=False,
        ),
        BoolInput(
            name="hybrid_search",
            display_name="Hybrid Search",
            info="Enable hybrid vector + graph search",
            value=True,
            required=False,
        ),
        IntInput(
            name="rrf_k",
            display_name="RRF Constant",
            info="Reciprocal Rank Fusion constant for score merging",
            value=60,
            required=False,
            advanced=True,
        ),
        MessageTextInput(
            name="text_property",
            display_name="Text Property",
            info="Property name for document text",
            value="text",
            required=False,
            advanced=True,
        ),
        MessageTextInput(
            name="embedding_property",
            display_name="Embedding Property",
            info="Property name for embeddings",
            value="embedding",
            required=False,
            advanced=True,
        ),
    ]

    outputs = [
        Output(
            display_name="Retriever",
            name="retriever",
            method="build_retriever",
        ),
    ]

    def build_retriever(self) -> NexusGraphRetriever:
        """Build and return a NexusGraphRetriever instance."""
        return NexusGraphRetriever(
            client=self.client,
            embedding=self.embedding,
            label=self.label or "Document",
            k=self.k or 4,
            vector_k=self.vector_k or 10,
            graph_depth=self.graph_depth or 1,
            hybrid_search=self.hybrid_search if self.hybrid_search is not None else True,
            rrf_k=self.rrf_k or 60,
            text_property=self.text_property or "text",
            embedding_property=self.embedding_property or "embedding",
        )


class NexusHybridSearchComponent(Component):
    """LangFlow component for Nexus Hybrid Search.

    This component performs hybrid search using vector similarity
    and graph traversal, returning ranked results.

    Inputs:
        retriever: NexusGraphRetriever instance
        query: Search query text

    Outputs:
        documents: Retrieved documents
        results: Results with RRF scores
    """

    display_name = "Nexus Hybrid Search"
    description = "Perform hybrid vector + graph search"
    icon = "search"
    name = "NexusHybridSearch"

    inputs = [
        HandleInput(
            name="retriever",
            display_name="Graph Retriever",
            info="NexusGraphRetriever instance",
            input_types=["NexusGraphRetriever", "BaseRetriever"],
            required=True,
        ),
        MessageTextInput(
            name="query",
            display_name="Search Query",
            info="Query text for hybrid search",
            required=True,
        ),
    ]

    outputs = [
        Output(
            display_name="Documents",
            name="documents",
            method="retrieve_documents",
        ),
        Output(
            display_name="Results with Scores",
            name="results",
            method="retrieve_with_scores",
        ),
    ]

    def retrieve_documents(self) -> List[Document]:
        """Retrieve relevant documents."""
        return self.retriever.invoke(self.query)

    def retrieve_with_scores(self) -> List[Data]:
        """Retrieve documents with RRF scores."""
        docs = self.retriever.invoke(self.query)

        return [
            Data(
                data={
                    "content": doc.page_content,
                    "metadata": doc.metadata,
                    "rrf_score": doc.metadata.get("_rrf_score"),
                    "node_id": doc.metadata.get("_node_id"),
                }
            )
            for doc in docs
        ]


class NexusGraphTraversalComponent(Component):
    """LangFlow component for Nexus Graph Traversal.

    This component allows custom graph traversal queries
    to explore relationships in the knowledge graph.

    Inputs:
        client: NexusClient instance
        start_node_id: Starting node ID
        relationship_types: Types of relationships to traverse
        max_depth: Maximum traversal depth
        return_properties: Properties to return

    Outputs:
        nodes: Traversed nodes
        paths: Traversal paths
    """

    display_name = "Nexus Graph Traversal"
    description = "Traverse graph relationships from a starting node"
    icon = "git-branch"
    name = "NexusGraphTraversal"

    inputs = [
        HandleInput(
            name="client",
            display_name="Nexus Client",
            info="NexusClient instance",
            input_types=["NexusClient"],
            required=True,
        ),
        IntInput(
            name="start_node_id",
            display_name="Start Node ID",
            info="ID of the starting node",
            required=True,
        ),
        MessageTextInput(
            name="relationship_types",
            display_name="Relationship Types",
            info="Comma-separated relationship types to traverse (empty for all)",
            required=False,
        ),
        IntInput(
            name="max_depth",
            display_name="Max Depth",
            info="Maximum traversal depth",
            value=2,
            required=False,
        ),
        MessageTextInput(
            name="label",
            display_name="Target Label",
            info="Optional label filter for target nodes",
            required=False,
        ),
    ]

    outputs = [
        Output(
            display_name="Connected Nodes",
            name="nodes",
            method="traverse_graph",
        ),
    ]

    def traverse_graph(self) -> List[Data]:
        """Traverse the graph and return connected nodes."""
        depth = self.max_depth or 2

        # Build relationship pattern
        if self.relationship_types:
            rel_types = [t.strip() for t in self.relationship_types.split(",")]
            rel_pattern = ":" + "|".join(rel_types)
        else:
            rel_pattern = ""

        # Build label filter
        label_filter = f":{self.label}" if self.label else ""

        query = f"""
        MATCH (start)-[r{rel_pattern}*1..{depth}]-(target{label_filter})
        WHERE id(start) = $start_id
        RETURN DISTINCT id(target) as node_id, target as node
        LIMIT 100
        """

        result = self.client.execute_cypher_sync(
            query,
            {"start_id": self.start_node_id},
        )

        nodes = []
        for row in result.get("rows", []):
            if len(row) >= 2:
                node_id = row[0]
                node_data = row[1] if isinstance(row[1], dict) else {}
                properties = node_data.get("properties", node_data)

                nodes.append(
                    Data(
                        data={
                            "node_id": node_id,
                            **properties,
                        }
                    )
                )

        return nodes
