"""Tests for NexusGraphRetriever."""

import pytest
from unittest.mock import AsyncMock, MagicMock

from langchain_nexus import NexusGraphRetriever, NexusClient


class MockEmbeddings:
    """Mock embeddings for testing."""

    def embed_query(self, text):
        return [0.1, 0.2, 0.3]

    async def aembed_query(self, text):
        return [0.1, 0.2, 0.3]


@pytest.fixture
def mock_client():
    """Create a mock NexusClient."""
    client = MagicMock(spec=NexusClient)
    client.knn_search_sync.return_value = [
        {
            "node_id": 1,
            "properties": {
                "text": "Hello world",
                "metadata_source": "test",
            },
            "score": 0.95,
        },
        {
            "node_id": 2,
            "properties": {
                "text": "Goodbye world",
            },
            "score": 0.85,
        },
    ]
    client.execute_cypher_sync.return_value = {
        "rows": [
            [3, {"properties": {"text": "Related content"}}],
            [4, {"properties": {"text": "More related"}}],
        ],
        "columns": ["node_id", "node"],
    }
    return client


@pytest.fixture
def mock_embeddings():
    """Create mock embeddings."""
    return MockEmbeddings()


@pytest.fixture
def retriever(mock_client, mock_embeddings):
    """Create a NexusGraphRetriever instance."""
    return NexusGraphRetriever(
        client=mock_client,
        embedding=mock_embeddings,
        k=4,
        vector_k=10,
        graph_depth=1,
        hybrid_search=True,
    )


def test_get_relevant_documents(retriever):
    """Test retrieving relevant documents."""
    docs = retriever._get_relevant_documents("Hello")

    assert len(docs) > 0
    assert docs[0].page_content in ["Hello world", "Goodbye world", "Related content", "More related"]


def test_vector_only_search(mock_client, mock_embeddings):
    """Test vector-only search (no hybrid)."""
    retriever = NexusGraphRetriever(
        client=mock_client,
        embedding=mock_embeddings,
        hybrid_search=False,
    )

    docs = retriever._get_relevant_documents("Hello")

    assert len(docs) == 2
    # Graph traversal should not be called
    mock_client.execute_cypher_sync.assert_not_called()


def test_rrf_score(retriever):
    """Test RRF score calculation."""
    score1 = retriever._rrf_score(1)
    score2 = retriever._rrf_score(2)

    # Higher rank should have higher score
    assert score1 > score2
    # Score formula: 1 / (k + rank)
    assert score1 == 1.0 / (60 + 1)


def test_merge_results(retriever):
    """Test merging vector and graph results."""
    vector_results = [
        {"node_id": 1, "properties": {"text": "A"}, "score": 0.9},
        {"node_id": 2, "properties": {"text": "B"}, "score": 0.8},
    ]
    graph_results = [
        {"node_id": 2, "properties": {"text": "B"}},  # Overlapping
        {"node_id": 3, "properties": {"text": "C"}},
    ]

    merged = retriever._merge_results(vector_results, graph_results)

    # Node 2 should have highest RRF score (appears in both)
    assert any(r["node_id"] == 2 for r in merged)
    # All should have rrf_score
    assert all("rrf_score" in r for r in merged)


def test_results_to_documents(retriever):
    """Test converting results to documents."""
    results = [
        {
            "node_id": 1,
            "properties": {"text": "Hello", "metadata_source": "test"},
            "score": 0.9,
            "rrf_score": 0.016,
        },
    ]

    docs = retriever._results_to_documents(results)

    assert len(docs) == 1
    assert docs[0].page_content == "Hello"
    assert docs[0].metadata["_node_id"] == 1
    assert docs[0].metadata["_score"] == 0.9
    assert docs[0].metadata["_rrf_score"] == 0.016
    assert docs[0].metadata["source"] == "test"


def test_traverse_graph(retriever, mock_client):
    """Test graph traversal."""
    results = retriever._traverse_graph([1, 2])

    assert len(results) == 2
    mock_client.execute_cypher_sync.assert_called_once()


def test_traverse_graph_empty(retriever):
    """Test graph traversal with empty node list."""
    results = retriever._traverse_graph([])
    assert results == []


def test_graph_depth_config(mock_client, mock_embeddings):
    """Test configurable graph depth."""
    retriever = NexusGraphRetriever(
        client=mock_client,
        embedding=mock_embeddings,
        graph_depth=3,
    )

    assert retriever.graph_depth == 3


def test_rrf_k_config(mock_client, mock_embeddings):
    """Test configurable RRF k constant."""
    retriever = NexusGraphRetriever(
        client=mock_client,
        embedding=mock_embeddings,
        rrf_k=100,
    )

    assert retriever.rrf_k == 100
    score = retriever._rrf_score(1)
    assert score == 1.0 / (100 + 1)


@pytest.mark.asyncio
async def test_async_get_relevant_documents(mock_client, mock_embeddings):
    """Test async document retrieval."""
    mock_client.knn_search = AsyncMock(
        return_value=[
            {"node_id": 1, "properties": {"text": "Hello"}, "score": 0.9}
        ]
    )
    mock_client.execute_cypher = AsyncMock(
        return_value={"rows": [], "columns": []}
    )

    retriever = NexusGraphRetriever(
        client=mock_client,
        embedding=mock_embeddings,
        hybrid_search=False,
    )

    docs = await retriever._aget_relevant_documents("Hello")

    assert len(docs) == 1
    assert docs[0].page_content == "Hello"


@pytest.mark.asyncio
async def test_async_traverse_graph(mock_client, mock_embeddings):
    """Test async graph traversal."""
    mock_client.knn_search = AsyncMock(
        return_value=[{"node_id": 1, "properties": {"text": "A"}, "score": 0.9}]
    )
    mock_client.execute_cypher = AsyncMock(
        return_value={
            "rows": [[2, {"properties": {"text": "B"}}]],
            "columns": ["node_id", "node"],
        }
    )

    retriever = NexusGraphRetriever(
        client=mock_client,
        embedding=mock_embeddings,
        hybrid_search=True,
    )

    results = await retriever._atraverse_graph([1])

    assert len(results) == 1
