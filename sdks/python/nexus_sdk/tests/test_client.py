"""Tests for NexusClient."""

import pytest
from nexus_sdk import NexusClient
from nexus_sdk.error import ConfigurationError


def test_client_creation():
    """Test creating a client."""
    client = NexusClient("http://localhost:15474")
    assert client.base_url == "http://localhost:15474"
    assert client.api_key is None
    assert client.username is None
    assert client.password is None


def test_client_with_api_key():
    """Test creating a client with API key."""
    client = NexusClient("http://localhost:15474", api_key="test-key")
    assert client.api_key == "test-key"


def test_client_with_credentials():
    """Test creating a client with username/password."""
    client = NexusClient(
        "http://localhost:15474", username="user", password="pass"
    )
    assert client.username == "user"
    assert client.password == "pass"


def test_client_url_normalization():
    """Test URL normalization."""
    client = NexusClient("http://localhost:15474/")
    assert client.base_url == "http://localhost:15474"


@pytest.mark.asyncio
async def test_client_context_manager():
    """Test client as context manager."""
    async with NexusClient("http://localhost:15474") as client:
        assert client.base_url == "http://localhost:15474"

