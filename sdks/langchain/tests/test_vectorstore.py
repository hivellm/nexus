"""Tests for NexusVectorStore."""

import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from langchain_core.documents import Document

from langchain_nexus import NexusVectorStore, NexusClient


class MockEmbeddings:
    """Mock embeddings for testing."""

    def embed_documents(self, texts):
        return [[0.1, 0.2, 0.3] for _ in texts]

    def embed_query(self, text):
        return [0.1, 0.2, 0.3]

    async def aembed_documents(self, texts):
        return [[0.1, 0.2, 0.3] for _ in texts]

    async def aembed_query(self, text):
        return [0.1, 0.2, 0.3]


@pytest.fixture
def mock_client():
    """Create a mock NexusClient."""
    client = MagicMock(spec=NexusClient)
    client.create_node_sync.return_value = 1
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
                "metadata_source": "test2",
            },
            "score": 0.85,
        },
    ]
    client.execute_cypher_sync.return_value = {"rows": [], "columns": []}
    return client


@pytest.fixture
def mock_embeddings():
    """Create mock embeddings."""
    return MockEmbeddings()


@pytest.fixture
def vectorstore(mock_client, mock_embeddings):
    """Create a NexusVectorStore instance."""
    return NexusVectorStore(mock_client, mock_embeddings)


def test_add_texts(vectorstore, mock_client):
    """Test adding texts to the vector store."""
    ids = vectorstore.add_texts(
        ["Hello world", "Goodbye world"],
        metadatas=[{"source": "test1"}, {"source": "test2"}],
    )

    assert len(ids) == 2
    assert mock_client.create_node_sync.call_count == 2


def test_add_texts_empty(vectorstore):
    """Test adding empty list of texts."""
    ids = vectorstore.add_texts([])
    assert ids == []


def test_add_documents(vectorstore, mock_client):
    """Test adding documents to the vector store."""
    docs = [
        Document(page_content="Hello world", metadata={"source": "test1"}),
        Document(page_content="Goodbye world", metadata={"source": "test2"}),
    ]
    ids = vectorstore.add_documents(docs)

    assert len(ids) == 2
    assert mock_client.create_node_sync.call_count == 2


def test_similarity_search(vectorstore):
    """Test similarity search."""
    docs = vectorstore.similarity_search("Hello", k=2)

    assert len(docs) == 2
    assert docs[0].page_content == "Hello world"
    assert docs[0].metadata["source"] == "test"


def test_similarity_search_with_score(vectorstore):
    """Test similarity search with scores."""
    results = vectorstore.similarity_search_with_score("Hello", k=2)

    assert len(results) == 2
    doc, score = results[0]
    assert doc.page_content == "Hello world"
    assert score == 0.95


def test_similarity_search_by_vector(vectorstore):
    """Test similarity search by vector."""
    docs = vectorstore.similarity_search_by_vector([0.1, 0.2, 0.3], k=2)

    assert len(docs) == 2
    assert docs[0].page_content == "Hello world"


def test_delete(vectorstore, mock_client):
    """Test deleting documents."""
    result = vectorstore.delete(["id1", "id2"])

    assert result is True
    assert mock_client.execute_cypher_sync.call_count == 2


def test_delete_empty(vectorstore):
    """Test deleting with empty list."""
    result = vectorstore.delete([])
    assert result is None


def test_embeddings_property(vectorstore, mock_embeddings):
    """Test embeddings property."""
    assert vectorstore.embeddings == mock_embeddings


def test_metadata_extraction(vectorstore):
    """Test metadata is correctly extracted from properties."""
    docs = vectorstore.similarity_search("test", k=1)

    assert "source" in docs[0].metadata
    assert docs[0].metadata["source"] == "test"
    assert "_node_id" in docs[0].metadata


def test_from_texts():
    """Test creating vector store from texts."""
    with patch.object(NexusClient, "__init__", return_value=None):
        with patch.object(NexusVectorStore, "add_texts", return_value=["id1", "id2"]):
            client = MagicMock(spec=NexusClient)
            with patch("langchain_nexus.vectorstore.NexusClient", return_value=client):
                embeddings = MockEmbeddings()
                vs = NexusVectorStore.from_texts(
                    texts=["Hello", "World"],
                    embedding=embeddings,
                    url="http://localhost:15474",
                )
                assert vs is not None


@pytest.mark.asyncio
async def test_async_add_texts(mock_client, mock_embeddings):
    """Test async add texts."""
    mock_client.create_node = AsyncMock(return_value=1)

    vectorstore = NexusVectorStore(mock_client, mock_embeddings)
    ids = await vectorstore.aadd_texts(["Hello", "World"])

    assert len(ids) == 2
    assert mock_client.create_node.call_count == 2


@pytest.mark.asyncio
async def test_async_similarity_search(mock_client, mock_embeddings):
    """Test async similarity search."""
    mock_client.knn_search = AsyncMock(
        return_value=[
            {
                "node_id": 1,
                "properties": {"text": "Hello world"},
                "score": 0.95,
            }
        ]
    )

    vectorstore = NexusVectorStore(mock_client, mock_embeddings)
    docs = await vectorstore.asimilarity_search("Hello")

    assert len(docs) == 1
    assert docs[0].page_content == "Hello world"
