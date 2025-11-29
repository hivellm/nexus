"""Nexus Graph Memory implementation for LangChain."""

from __future__ import annotations

import time
from typing import Any, Dict, List, Optional

from langchain_core.chat_history import BaseChatMessageHistory
from langchain_core.messages import (
    AIMessage,
    BaseMessage,
    HumanMessage,
    SystemMessage,
    messages_from_dict,
    messages_to_dict,
)
from pydantic import Field

from langchain_nexus.client import NexusClient


class NexusGraphMemory(BaseChatMessageHistory):
    """Graph-based conversation memory using Nexus.

    This memory implementation stores conversation messages as nodes in a graph,
    connected by FOLLOWS relationships. It enables context-aware memory retrieval
    and conversation analysis through graph queries.

    Args:
        client: NexusClient instance for database connection
        session_id: Unique identifier for the conversation session
        user_id: Optional user identifier for multi-user scenarios
        window_size: Number of recent messages to return (default: 10)
        message_label: Node label for message nodes (default: "Message")
        session_label: Node label for session nodes (default: "Session")

    Example:
        >>> from langchain_nexus import NexusGraphMemory, NexusClient
        >>>
        >>> client = NexusClient("http://localhost:15474")
        >>> memory = NexusGraphMemory(
        ...     client=client,
        ...     session_id="conversation-123",
        ... )
        >>>
        >>> memory.add_user_message("Hello, how are you?")
        >>> memory.add_ai_message("I'm doing great, thanks for asking!")
        >>>
        >>> messages = memory.messages
    """

    client: NexusClient = Field(description="Nexus database client")
    session_id: str = Field(description="Unique session identifier")
    user_id: Optional[str] = Field(default=None, description="Optional user identifier")
    window_size: int = Field(default=10, description="Number of messages to return")
    message_label: str = Field(default="Message", description="Message node label")
    session_label: str = Field(default="Session", description="Session node label")
    _session_node_id: Optional[int] = None
    _message_count: int = 0

    class Config:
        arbitrary_types_allowed = True

    def __init__(self, **data: Any):
        super().__init__(**data)
        self._ensure_session()

    def _ensure_session(self) -> None:
        """Ensure session node exists."""
        query = f"""
        MERGE (s:{self.session_label} {{session_id: $session_id}})
        ON CREATE SET s.created_at = $timestamp, s.user_id = $user_id
        RETURN id(s) as node_id
        """
        result = self.client.execute_cypher_sync(
            query,
            {
                "session_id": self.session_id,
                "timestamp": int(time.time() * 1000),
                "user_id": self.user_id,
            },
        )
        rows = result.get("rows", [])
        if rows and len(rows[0]) > 0:
            self._session_node_id = rows[0][0]

    @property
    def messages(self) -> List[BaseMessage]:
        """Retrieve recent messages from the conversation.

        Returns:
            List of messages in chronological order
        """
        query = f"""
        MATCH (s:{self.session_label} {{session_id: $session_id}})-[:HAS_MESSAGE]->(m:{self.message_label})
        RETURN m.type as type, m.content as content, m.additional_kwargs as kwargs
        ORDER BY m.timestamp ASC
        LIMIT $limit
        """
        result = self.client.execute_cypher_sync(
            query,
            {"session_id": self.session_id, "limit": self.window_size},
        )

        messages = []
        for row in result.get("rows", []):
            if len(row) >= 2:
                msg_type = row[0]
                content = row[1]
                kwargs = row[2] if len(row) > 2 and row[2] else {}

                if msg_type == "human":
                    messages.append(HumanMessage(content=content, additional_kwargs=kwargs))
                elif msg_type == "ai":
                    messages.append(AIMessage(content=content, additional_kwargs=kwargs))
                elif msg_type == "system":
                    messages.append(SystemMessage(content=content, additional_kwargs=kwargs))

        return messages

    def add_message(self, message: BaseMessage) -> None:
        """Add a message to the conversation history.

        Args:
            message: Message to add
        """
        # Determine message type
        if isinstance(message, HumanMessage):
            msg_type = "human"
        elif isinstance(message, AIMessage):
            msg_type = "ai"
        elif isinstance(message, SystemMessage):
            msg_type = "system"
        else:
            msg_type = "unknown"

        timestamp = int(time.time() * 1000)

        # Create message node and link to session
        query = f"""
        MATCH (s:{self.session_label} {{session_id: $session_id}})
        CREATE (m:{self.message_label} {{
            type: $type,
            content: $content,
            timestamp: $timestamp,
            sequence: $sequence
        }})
        CREATE (s)-[:HAS_MESSAGE]->(m)
        WITH m
        OPTIONAL MATCH (prev:{self.message_label})<-[:HAS_MESSAGE]-(:{self.session_label} {{session_id: $session_id}})
        WHERE prev.sequence = $prev_sequence
        FOREACH (p IN CASE WHEN prev IS NOT NULL THEN [prev] ELSE [] END |
            CREATE (p)-[:FOLLOWED_BY]->(m)
        )
        RETURN id(m) as node_id
        """

        self._message_count += 1
        self.client.execute_cypher_sync(
            query,
            {
                "session_id": self.session_id,
                "type": msg_type,
                "content": message.content,
                "timestamp": timestamp,
                "sequence": self._message_count,
                "prev_sequence": self._message_count - 1,
            },
        )

    def add_user_message(self, message: str) -> None:
        """Add a user message to the conversation.

        Args:
            message: User message content
        """
        self.add_message(HumanMessage(content=message))

    def add_ai_message(self, message: str) -> None:
        """Add an AI message to the conversation.

        Args:
            message: AI message content
        """
        self.add_message(AIMessage(content=message))

    def clear(self) -> None:
        """Clear all messages from the conversation."""
        query = f"""
        MATCH (s:{self.session_label} {{session_id: $session_id}})-[:HAS_MESSAGE]->(m:{self.message_label})
        DETACH DELETE m
        """
        self.client.execute_cypher_sync(query, {"session_id": self.session_id})
        self._message_count = 0

    def get_messages_by_type(self, msg_type: str) -> List[BaseMessage]:
        """Get messages of a specific type.

        Args:
            msg_type: Message type ("human", "ai", "system")

        Returns:
            List of messages of the specified type
        """
        query = f"""
        MATCH (s:{self.session_label} {{session_id: $session_id}})-[:HAS_MESSAGE]->(m:{self.message_label})
        WHERE m.type = $type
        RETURN m.type as type, m.content as content
        ORDER BY m.timestamp ASC
        """
        result = self.client.execute_cypher_sync(
            query,
            {"session_id": self.session_id, "type": msg_type},
        )

        messages = []
        for row in result.get("rows", []):
            if len(row) >= 2:
                content = row[1]
                if msg_type == "human":
                    messages.append(HumanMessage(content=content))
                elif msg_type == "ai":
                    messages.append(AIMessage(content=content))
                elif msg_type == "system":
                    messages.append(SystemMessage(content=content))

        return messages

    def get_conversation_summary(self) -> Dict[str, Any]:
        """Get a summary of the conversation.

        Returns:
            Dictionary with conversation statistics
        """
        query = f"""
        MATCH (s:{self.session_label} {{session_id: $session_id}})-[:HAS_MESSAGE]->(m:{self.message_label})
        RETURN
            count(m) as total_messages,
            count(CASE WHEN m.type = 'human' THEN 1 END) as human_messages,
            count(CASE WHEN m.type = 'ai' THEN 1 END) as ai_messages,
            min(m.timestamp) as first_message_time,
            max(m.timestamp) as last_message_time
        """
        result = self.client.execute_cypher_sync(
            query, {"session_id": self.session_id}
        )

        rows = result.get("rows", [])
        if rows and len(rows[0]) >= 5:
            row = rows[0]
            return {
                "total_messages": row[0] or 0,
                "human_messages": row[1] or 0,
                "ai_messages": row[2] or 0,
                "first_message_time": row[3],
                "last_message_time": row[4],
                "session_id": self.session_id,
                "user_id": self.user_id,
            }

        return {
            "total_messages": 0,
            "human_messages": 0,
            "ai_messages": 0,
            "first_message_time": None,
            "last_message_time": None,
            "session_id": self.session_id,
            "user_id": self.user_id,
        }

    def search_messages(self, keyword: str, limit: int = 10) -> List[BaseMessage]:
        """Search messages containing a keyword.

        Args:
            keyword: Keyword to search for
            limit: Maximum number of results

        Returns:
            List of matching messages
        """
        query = f"""
        MATCH (s:{self.session_label} {{session_id: $session_id}})-[:HAS_MESSAGE]->(m:{self.message_label})
        WHERE m.content CONTAINS $keyword
        RETURN m.type as type, m.content as content
        ORDER BY m.timestamp DESC
        LIMIT $limit
        """
        result = self.client.execute_cypher_sync(
            query,
            {"session_id": self.session_id, "keyword": keyword, "limit": limit},
        )

        messages = []
        for row in result.get("rows", []):
            if len(row) >= 2:
                msg_type = row[0]
                content = row[1]

                if msg_type == "human":
                    messages.append(HumanMessage(content=content))
                elif msg_type == "ai":
                    messages.append(AIMessage(content=content))
                elif msg_type == "system":
                    messages.append(SystemMessage(content=content))

        return messages

    async def aget_messages(self) -> List[BaseMessage]:
        """Retrieve recent messages asynchronously.

        Returns:
            List of messages in chronological order
        """
        query = f"""
        MATCH (s:{self.session_label} {{session_id: $session_id}})-[:HAS_MESSAGE]->(m:{self.message_label})
        RETURN m.type as type, m.content as content, m.additional_kwargs as kwargs
        ORDER BY m.timestamp ASC
        LIMIT $limit
        """
        result = await self.client.execute_cypher(
            query,
            {"session_id": self.session_id, "limit": self.window_size},
        )

        messages = []
        for row in result.get("rows", []):
            if len(row) >= 2:
                msg_type = row[0]
                content = row[1]
                kwargs = row[2] if len(row) > 2 and row[2] else {}

                if msg_type == "human":
                    messages.append(HumanMessage(content=content, additional_kwargs=kwargs))
                elif msg_type == "ai":
                    messages.append(AIMessage(content=content, additional_kwargs=kwargs))
                elif msg_type == "system":
                    messages.append(SystemMessage(content=content, additional_kwargs=kwargs))

        return messages

    async def aadd_message(self, message: BaseMessage) -> None:
        """Add a message asynchronously.

        Args:
            message: Message to add
        """
        if isinstance(message, HumanMessage):
            msg_type = "human"
        elif isinstance(message, AIMessage):
            msg_type = "ai"
        elif isinstance(message, SystemMessage):
            msg_type = "system"
        else:
            msg_type = "unknown"

        timestamp = int(time.time() * 1000)

        query = f"""
        MATCH (s:{self.session_label} {{session_id: $session_id}})
        CREATE (m:{self.message_label} {{
            type: $type,
            content: $content,
            timestamp: $timestamp,
            sequence: $sequence
        }})
        CREATE (s)-[:HAS_MESSAGE]->(m)
        RETURN id(m) as node_id
        """

        self._message_count += 1
        await self.client.execute_cypher(
            query,
            {
                "session_id": self.session_id,
                "type": msg_type,
                "content": message.content,
                "timestamp": timestamp,
                "sequence": self._message_count,
            },
        )

    async def aclear(self) -> None:
        """Clear all messages asynchronously."""
        query = f"""
        MATCH (s:{self.session_label} {{session_id: $session_id}})-[:HAS_MESSAGE]->(m:{self.message_label})
        DETACH DELETE m
        """
        await self.client.execute_cypher(query, {"session_id": self.session_id})
        self._message_count = 0
