"""Tests for NexusGraphMemory."""

import pytest
from unittest.mock import AsyncMock, MagicMock

from langchain_core.messages import AIMessage, HumanMessage, SystemMessage

from langchain_nexus import NexusGraphMemory, NexusClient


@pytest.fixture
def mock_client():
    """Create a mock NexusClient."""
    client = MagicMock(spec=NexusClient)
    # Mock session creation
    client.execute_cypher_sync.return_value = {
        "rows": [[1]],
        "columns": ["node_id"],
    }
    return client


@pytest.fixture
def memory(mock_client):
    """Create a NexusGraphMemory instance."""
    return NexusGraphMemory(
        client=mock_client,
        session_id="test-session",
        user_id="test-user",
    )


def test_init_creates_session(mock_client):
    """Test that initialization creates a session."""
    memory = NexusGraphMemory(
        client=mock_client,
        session_id="test-session",
    )

    # Should have called execute_cypher_sync for MERGE session
    mock_client.execute_cypher_sync.assert_called()


def test_add_user_message(memory, mock_client):
    """Test adding a user message."""
    # Reset call count after init
    mock_client.execute_cypher_sync.reset_mock()

    memory.add_user_message("Hello, how are you?")

    # Should call execute_cypher_sync to create message
    assert mock_client.execute_cypher_sync.called


def test_add_ai_message(memory, mock_client):
    """Test adding an AI message."""
    mock_client.execute_cypher_sync.reset_mock()

    memory.add_ai_message("I'm doing great!")

    assert mock_client.execute_cypher_sync.called


def test_add_message_human(memory, mock_client):
    """Test adding a HumanMessage."""
    mock_client.execute_cypher_sync.reset_mock()

    memory.add_message(HumanMessage(content="Test message"))

    assert mock_client.execute_cypher_sync.called


def test_add_message_ai(memory, mock_client):
    """Test adding an AIMessage."""
    mock_client.execute_cypher_sync.reset_mock()

    memory.add_message(AIMessage(content="AI response"))

    assert mock_client.execute_cypher_sync.called


def test_add_message_system(memory, mock_client):
    """Test adding a SystemMessage."""
    mock_client.execute_cypher_sync.reset_mock()

    memory.add_message(SystemMessage(content="System prompt"))

    assert mock_client.execute_cypher_sync.called


def test_get_messages(mock_client):
    """Test retrieving messages."""
    mock_client.execute_cypher_sync.side_effect = [
        {"rows": [[1]], "columns": ["node_id"]},  # Session creation
        {
            "rows": [
                ["human", "Hello", {}],
                ["ai", "Hi there!", {}],
            ],
            "columns": ["type", "content", "kwargs"],
        },
    ]

    memory = NexusGraphMemory(
        client=mock_client,
        session_id="test-session",
    )

    messages = memory.messages

    assert len(messages) == 2
    assert isinstance(messages[0], HumanMessage)
    assert messages[0].content == "Hello"
    assert isinstance(messages[1], AIMessage)
    assert messages[1].content == "Hi there!"


def test_clear(memory, mock_client):
    """Test clearing messages."""
    mock_client.execute_cypher_sync.reset_mock()

    memory.clear()

    assert mock_client.execute_cypher_sync.called
    assert memory._message_count == 0


def test_get_messages_by_type(mock_client):
    """Test getting messages by type."""
    mock_client.execute_cypher_sync.side_effect = [
        {"rows": [[1]], "columns": ["node_id"]},  # Session creation
        {
            "rows": [
                ["human", "Message 1"],
                ["human", "Message 2"],
            ],
            "columns": ["type", "content"],
        },
    ]

    memory = NexusGraphMemory(
        client=mock_client,
        session_id="test-session",
    )

    messages = memory.get_messages_by_type("human")

    assert len(messages) == 2
    assert all(isinstance(m, HumanMessage) for m in messages)


def test_get_conversation_summary(mock_client):
    """Test getting conversation summary."""
    mock_client.execute_cypher_sync.side_effect = [
        {"rows": [[1]], "columns": ["node_id"]},  # Session creation
        {
            "rows": [[10, 5, 5, 1700000000000, 1700000001000]],
            "columns": ["total", "human", "ai", "first", "last"],
        },
    ]

    memory = NexusGraphMemory(
        client=mock_client,
        session_id="test-session",
        user_id="test-user",
    )

    summary = memory.get_conversation_summary()

    assert summary["total_messages"] == 10
    assert summary["human_messages"] == 5
    assert summary["ai_messages"] == 5
    assert summary["session_id"] == "test-session"
    assert summary["user_id"] == "test-user"


def test_search_messages(mock_client):
    """Test searching messages by keyword."""
    mock_client.execute_cypher_sync.side_effect = [
        {"rows": [[1]], "columns": ["node_id"]},  # Session creation
        {
            "rows": [["human", "Hello world"]],
            "columns": ["type", "content"],
        },
    ]

    memory = NexusGraphMemory(
        client=mock_client,
        session_id="test-session",
    )

    messages = memory.search_messages("Hello")

    assert len(messages) == 1
    assert messages[0].content == "Hello world"


def test_window_size(mock_client):
    """Test window size configuration."""
    memory = NexusGraphMemory(
        client=mock_client,
        session_id="test-session",
        window_size=20,
    )

    assert memory.window_size == 20


def test_custom_labels(mock_client):
    """Test custom node labels."""
    memory = NexusGraphMemory(
        client=mock_client,
        session_id="test-session",
        message_label="ChatMessage",
        session_label="Conversation",
    )

    assert memory.message_label == "ChatMessage"
    assert memory.session_label == "Conversation"


@pytest.mark.asyncio
async def test_async_get_messages(mock_client):
    """Test async message retrieval."""
    mock_client.execute_cypher_sync.return_value = {
        "rows": [[1]],
        "columns": ["node_id"],
    }
    mock_client.execute_cypher = AsyncMock(
        return_value={
            "rows": [["human", "Hello", {}]],
            "columns": ["type", "content", "kwargs"],
        }
    )

    memory = NexusGraphMemory(
        client=mock_client,
        session_id="test-session",
    )

    messages = await memory.aget_messages()

    assert len(messages) == 1
    assert isinstance(messages[0], HumanMessage)


@pytest.mark.asyncio
async def test_async_add_message(mock_client):
    """Test async message addition."""
    mock_client.execute_cypher_sync.return_value = {
        "rows": [[1]],
        "columns": ["node_id"],
    }
    mock_client.execute_cypher = AsyncMock(
        return_value={"rows": [[2]], "columns": ["node_id"]}
    )

    memory = NexusGraphMemory(
        client=mock_client,
        session_id="test-session",
    )

    await memory.aadd_message(HumanMessage(content="Async message"))

    mock_client.execute_cypher.assert_called()


@pytest.mark.asyncio
async def test_async_clear(mock_client):
    """Test async clear."""
    mock_client.execute_cypher_sync.return_value = {
        "rows": [[1]],
        "columns": ["node_id"],
    }
    mock_client.execute_cypher = AsyncMock(
        return_value={"rows": [], "columns": []}
    )

    memory = NexusGraphMemory(
        client=mock_client,
        session_id="test-session",
    )

    await memory.aclear()

    mock_client.execute_cypher.assert_called()
    assert memory._message_count == 0
