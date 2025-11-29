"""Nexus Graph Memory Component for LangFlow."""

from typing import List

from langflow.custom import Component
from langflow.io import (
    HandleInput,
    IntInput,
    MessageTextInput,
    Output,
)
from langflow.schema import Data

from langchain_nexus import NexusGraphMemory, NexusClient


class NexusGraphMemoryComponent(Component):
    """LangFlow component for Nexus Graph Memory.

    This component provides graph-based conversation memory
    that stores messages as nodes with relationships.

    Inputs:
        client: NexusClient from NexusConnection component
        session_id: Unique conversation session identifier
        user_id: Optional user identifier
        window_size: Number of recent messages to retrieve

    Outputs:
        memory: NexusGraphMemory instance
        messages: Recent conversation messages
    """

    display_name = "Nexus Graph Memory"
    description = "Graph-based conversation memory using Nexus"
    icon = "message-circle"
    name = "NexusGraphMemory"

    inputs = [
        HandleInput(
            name="client",
            display_name="Nexus Client",
            info="NexusClient from NexusConnection component",
            input_types=["NexusClient"],
            required=True,
        ),
        MessageTextInput(
            name="session_id",
            display_name="Session ID",
            info="Unique identifier for the conversation session",
            required=True,
        ),
        MessageTextInput(
            name="user_id",
            display_name="User ID",
            info="Optional user identifier for multi-user scenarios",
            required=False,
        ),
        IntInput(
            name="window_size",
            display_name="Window Size",
            info="Number of recent messages to retrieve",
            value=10,
            required=False,
        ),
        MessageTextInput(
            name="message_label",
            display_name="Message Label",
            info="Node label for message nodes",
            value="Message",
            required=False,
            advanced=True,
        ),
        MessageTextInput(
            name="session_label",
            display_name="Session Label",
            info="Node label for session nodes",
            value="Session",
            required=False,
            advanced=True,
        ),
    ]

    outputs = [
        Output(
            display_name="Memory",
            name="memory",
            method="build_memory",
        ),
        Output(
            display_name="Messages",
            name="messages",
            method="get_messages",
        ),
        Output(
            display_name="Summary",
            name="summary",
            method="get_summary",
        ),
    ]

    def build_memory(self) -> NexusGraphMemory:
        """Build and return a NexusGraphMemory instance."""
        return NexusGraphMemory(
            client=self.client,
            session_id=self.session_id,
            user_id=self.user_id if self.user_id else None,
            window_size=self.window_size or 10,
            message_label=self.message_label or "Message",
            session_label=self.session_label or "Session",
        )

    def get_messages(self) -> List[Data]:
        """Get recent messages from the conversation."""
        memory = self.build_memory()
        messages = memory.messages

        return [
            Data(
                data={
                    "type": msg.type,
                    "content": msg.content,
                }
            )
            for msg in messages
        ]

    def get_summary(self) -> Data:
        """Get conversation summary."""
        memory = self.build_memory()
        summary = memory.get_conversation_summary()

        return Data(data=summary)


class NexusAddMessageComponent(Component):
    """LangFlow component for adding messages to Nexus Graph Memory.

    This component adds user or AI messages to the conversation
    history stored in the graph.

    Inputs:
        memory: NexusGraphMemory instance
        message: Message content to add
        message_type: Type of message (human, ai, system)

    Outputs:
        status: Operation status
    """

    display_name = "Nexus Add Message"
    description = "Add a message to the graph-based conversation memory"
    icon = "plus-circle"
    name = "NexusAddMessage"

    inputs = [
        HandleInput(
            name="memory",
            display_name="Memory",
            info="NexusGraphMemory instance",
            input_types=["NexusGraphMemory", "BaseChatMessageHistory"],
            required=True,
        ),
        MessageTextInput(
            name="message",
            display_name="Message",
            info="Message content to add",
            required=True,
        ),
        MessageTextInput(
            name="message_type",
            display_name="Message Type",
            info="Type of message: human, ai, or system",
            value="human",
            required=False,
        ),
    ]

    outputs = [
        Output(
            display_name="Status",
            name="status",
            method="add_message",
        ),
    ]

    def add_message(self) -> Data:
        """Add message to the conversation memory."""
        try:
            msg_type = self.message_type or "human"

            if msg_type == "human":
                self.memory.add_user_message(self.message)
            elif msg_type == "ai":
                self.memory.add_ai_message(self.message)
            else:
                from langchain_core.messages import SystemMessage
                self.memory.add_message(SystemMessage(content=self.message))

            return Data(
                data={
                    "status": "success",
                    "message_type": msg_type,
                    "content": self.message,
                }
            )
        except Exception as e:
            return Data(
                data={
                    "status": "error",
                    "error": str(e),
                }
            )


class NexusSearchMessagesComponent(Component):
    """LangFlow component for searching messages in Nexus Graph Memory.

    This component searches conversation history for messages
    containing a specific keyword.

    Inputs:
        memory: NexusGraphMemory instance
        keyword: Keyword to search for
        limit: Maximum number of results

    Outputs:
        messages: Matching messages
    """

    display_name = "Nexus Search Messages"
    description = "Search conversation history for messages"
    icon = "search"
    name = "NexusSearchMessages"

    inputs = [
        HandleInput(
            name="memory",
            display_name="Memory",
            info="NexusGraphMemory instance",
            input_types=["NexusGraphMemory", "BaseChatMessageHistory"],
            required=True,
        ),
        MessageTextInput(
            name="keyword",
            display_name="Keyword",
            info="Keyword to search for in messages",
            required=True,
        ),
        IntInput(
            name="limit",
            display_name="Limit",
            info="Maximum number of results",
            value=10,
            required=False,
        ),
    ]

    outputs = [
        Output(
            display_name="Messages",
            name="messages",
            method="search_messages",
        ),
    ]

    def search_messages(self) -> List[Data]:
        """Search messages by keyword."""
        messages = self.memory.search_messages(
            keyword=self.keyword,
            limit=self.limit or 10,
        )

        return [
            Data(
                data={
                    "type": msg.type,
                    "content": msg.content,
                }
            )
            for msg in messages
        ]


class NexusClearMemoryComponent(Component):
    """LangFlow component for clearing Nexus Graph Memory.

    This component clears all messages from a conversation session.

    Inputs:
        memory: NexusGraphMemory instance

    Outputs:
        status: Operation status
    """

    display_name = "Nexus Clear Memory"
    description = "Clear all messages from a conversation session"
    icon = "trash-2"
    name = "NexusClearMemory"

    inputs = [
        HandleInput(
            name="memory",
            display_name="Memory",
            info="NexusGraphMemory instance",
            input_types=["NexusGraphMemory", "BaseChatMessageHistory"],
            required=True,
        ),
    ]

    outputs = [
        Output(
            display_name="Status",
            name="status",
            method="clear_memory",
        ),
    ]

    def clear_memory(self) -> Data:
        """Clear all messages from the conversation."""
        try:
            self.memory.clear()
            return Data(
                data={
                    "status": "success",
                    "message": "Memory cleared successfully",
                }
            )
        except Exception as e:
            return Data(
                data={
                    "status": "error",
                    "error": str(e),
                }
            )
