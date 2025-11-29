"""Tests for NexusClient."""

import pytest
from unittest.mock import AsyncMock, MagicMock, patch

from langchain_nexus import NexusClient


@pytest.fixture
def client():
    """Create a NexusClient instance."""
    return NexusClient(
        url="http://localhost:15474",
        api_key="test-api-key",
    )


def test_init_with_url():
    """Test client initialization with URL."""
    client = NexusClient("http://localhost:15474")
    assert client.url == "http://localhost:15474"


def test_init_with_trailing_slash():
    """Test URL normalization."""
    client = NexusClient("http://localhost:15474/")
    assert client.url == "http://localhost:15474"


def test_init_with_api_key():
    """Test client initialization with API key."""
    client = NexusClient(
        url="http://localhost:15474",
        api_key="my-api-key",
    )
    assert client.api_key == "my-api-key"


def test_init_with_basic_auth():
    """Test client initialization with basic auth."""
    client = NexusClient(
        url="http://localhost:15474",
        username="admin",
        password="password",
    )
    assert client.username == "admin"
    assert client.password == "password"


def test_init_with_timeout():
    """Test client initialization with custom timeout."""
    client = NexusClient(
        url="http://localhost:15474",
        timeout=60.0,
    )
    assert client.timeout == 60.0


def test_get_auth_headers_api_key():
    """Test auth headers with API key."""
    client = NexusClient(
        url="http://localhost:15474",
        api_key="my-api-key",
    )
    headers = client._get_auth_headers()

    assert headers["Content-Type"] == "application/json"
    assert headers["X-API-Key"] == "my-api-key"


def test_get_auth_headers_basic_auth():
    """Test auth headers with basic auth."""
    client = NexusClient(
        url="http://localhost:15474",
        username="admin",
        password="password",
    )
    headers = client._get_auth_headers()

    assert headers["Content-Type"] == "application/json"
    assert "Authorization" in headers
    assert headers["Authorization"].startswith("Basic ")


def test_get_auth_headers_no_auth():
    """Test auth headers without authentication."""
    client = NexusClient("http://localhost:15474")
    headers = client._get_auth_headers()

    assert headers["Content-Type"] == "application/json"
    assert "X-API-Key" not in headers
    assert "Authorization" not in headers


@pytest.mark.asyncio
async def test_execute_cypher():
    """Test async Cypher query execution."""
    client = NexusClient("http://localhost:15474")

    mock_response = MagicMock()
    mock_response.json.return_value = {
        "columns": ["count"],
        "rows": [[42]],
    }
    mock_response.raise_for_status = MagicMock()

    with patch.object(client, "_get_async_client") as mock_get_client:
        mock_http_client = AsyncMock()
        mock_http_client.post.return_value = mock_response
        mock_get_client.return_value = mock_http_client

        result = await client.execute_cypher("MATCH (n) RETURN count(n)")

        assert result["columns"] == ["count"]
        assert result["rows"] == [[42]]


def test_execute_cypher_sync():
    """Test sync Cypher query execution."""
    client = NexusClient("http://localhost:15474")

    mock_response = MagicMock()
    mock_response.json.return_value = {
        "columns": ["count"],
        "rows": [[42]],
    }
    mock_response.raise_for_status = MagicMock()

    with patch.object(client, "_get_sync_client") as mock_get_client:
        mock_http_client = MagicMock()
        mock_http_client.post.return_value = mock_response
        mock_get_client.return_value = mock_http_client

        result = client.execute_cypher_sync("MATCH (n) RETURN count(n)")

        assert result["columns"] == ["count"]
        assert result["rows"] == [[42]]


@pytest.mark.asyncio
async def test_knn_search():
    """Test async KNN search."""
    client = NexusClient("http://localhost:15474")

    mock_response = MagicMock()
    mock_response.json.return_value = {
        "results": [
            {"node_id": 1, "score": 0.9},
            {"node_id": 2, "score": 0.8},
        ]
    }
    mock_response.raise_for_status = MagicMock()

    with patch.object(client, "_get_async_client") as mock_get_client:
        mock_http_client = AsyncMock()
        mock_http_client.post.return_value = mock_response
        mock_get_client.return_value = mock_http_client

        results = await client.knn_search(
            label="Document",
            vector=[0.1, 0.2, 0.3],
            k=5,
        )

        assert len(results) == 2
        assert results[0]["node_id"] == 1


def test_knn_search_sync():
    """Test sync KNN search."""
    client = NexusClient("http://localhost:15474")

    mock_response = MagicMock()
    mock_response.json.return_value = {
        "results": [
            {"node_id": 1, "score": 0.9},
        ]
    }
    mock_response.raise_for_status = MagicMock()

    with patch.object(client, "_get_sync_client") as mock_get_client:
        mock_http_client = MagicMock()
        mock_http_client.post.return_value = mock_response
        mock_get_client.return_value = mock_http_client

        results = client.knn_search_sync(
            label="Document",
            vector=[0.1, 0.2, 0.3],
            k=5,
        )

        assert len(results) == 1


@pytest.mark.asyncio
async def test_create_node():
    """Test async node creation."""
    client = NexusClient("http://localhost:15474")

    mock_response = MagicMock()
    mock_response.json.return_value = {"node_id": 123}
    mock_response.raise_for_status = MagicMock()

    with patch.object(client, "_get_async_client") as mock_get_client:
        mock_http_client = AsyncMock()
        mock_http_client.post.return_value = mock_response
        mock_get_client.return_value = mock_http_client

        node_id = await client.create_node(
            labels=["Document"],
            properties={"text": "Hello"},
        )

        assert node_id == 123


def test_create_node_sync():
    """Test sync node creation."""
    client = NexusClient("http://localhost:15474")

    mock_response = MagicMock()
    mock_response.json.return_value = {"node_id": 456}
    mock_response.raise_for_status = MagicMock()

    with patch.object(client, "_get_sync_client") as mock_get_client:
        mock_http_client = MagicMock()
        mock_http_client.post.return_value = mock_response
        mock_get_client.return_value = mock_http_client

        node_id = client.create_node_sync(
            labels=["Document"],
            properties={"text": "Hello"},
        )

        assert node_id == 456


@pytest.mark.asyncio
async def test_create_relationship():
    """Test async relationship creation."""
    client = NexusClient("http://localhost:15474")

    mock_response = MagicMock()
    mock_response.json.return_value = {"relationship_id": 789}
    mock_response.raise_for_status = MagicMock()

    with patch.object(client, "_get_async_client") as mock_get_client:
        mock_http_client = AsyncMock()
        mock_http_client.post.return_value = mock_response
        mock_get_client.return_value = mock_http_client

        rel_id = await client.create_relationship(
            source_id=1,
            target_id=2,
            rel_type="KNOWS",
            properties={"since": 2020},
        )

        assert rel_id == 789


@pytest.mark.asyncio
async def test_health_check_success():
    """Test health check success."""
    client = NexusClient("http://localhost:15474")

    mock_response = MagicMock()
    mock_response.status_code = 200

    with patch.object(client, "_get_async_client") as mock_get_client:
        mock_http_client = AsyncMock()
        mock_http_client.get.return_value = mock_response
        mock_get_client.return_value = mock_http_client

        healthy = await client.health_check()

        assert healthy is True


@pytest.mark.asyncio
async def test_health_check_failure():
    """Test health check failure."""
    client = NexusClient("http://localhost:15474")

    with patch.object(client, "_get_async_client") as mock_get_client:
        mock_http_client = AsyncMock()
        mock_http_client.get.side_effect = Exception("Connection failed")
        mock_get_client.return_value = mock_http_client

        healthy = await client.health_check()

        assert healthy is False


def test_health_check_sync_success():
    """Test sync health check success."""
    client = NexusClient("http://localhost:15474")

    mock_response = MagicMock()
    mock_response.status_code = 200

    with patch.object(client, "_get_sync_client") as mock_get_client:
        mock_http_client = MagicMock()
        mock_http_client.get.return_value = mock_response
        mock_get_client.return_value = mock_http_client

        healthy = client.health_check_sync()

        assert healthy is True


@pytest.mark.asyncio
async def test_close():
    """Test closing the client."""
    client = NexusClient("http://localhost:15474")

    mock_async_client = AsyncMock()
    mock_sync_client = MagicMock()

    client._client = mock_async_client
    client._sync_client = mock_sync_client

    await client.close()

    mock_async_client.aclose.assert_called_once()
    mock_sync_client.close.assert_called_once()
    assert client._client is None
    assert client._sync_client is None
