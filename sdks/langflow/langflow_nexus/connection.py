"""Nexus Connection Component for LangFlow."""

from typing import Optional

from langflow.custom import Component
from langflow.io import MessageTextInput, SecretStrInput, Output
from langflow.schema import Data

from langchain_nexus import NexusClient


class NexusConnectionComponent(Component):
    """LangFlow component for Nexus database connection.

    This component creates a connection to a Nexus graph database
    that can be used by other Nexus components.

    Inputs:
        url: Nexus server URL (e.g., http://localhost:15474)
        api_key: Optional API key for authentication
        username: Optional username for basic auth
        password: Optional password for basic auth
        timeout: Request timeout in seconds

    Outputs:
        client: NexusClient instance for database operations
    """

    display_name = "Nexus Connection"
    description = "Connect to a Nexus graph database server"
    icon = "database"
    name = "NexusConnection"

    inputs = [
        MessageTextInput(
            name="url",
            display_name="Server URL",
            info="Nexus server URL (e.g., http://localhost:15474)",
            value="http://localhost:15474",
            required=True,
        ),
        SecretStrInput(
            name="api_key",
            display_name="API Key",
            info="Optional API key for authentication",
            required=False,
        ),
        MessageTextInput(
            name="username",
            display_name="Username",
            info="Optional username for basic authentication",
            required=False,
        ),
        SecretStrInput(
            name="password",
            display_name="Password",
            info="Optional password for basic authentication",
            required=False,
        ),
        MessageTextInput(
            name="timeout",
            display_name="Timeout",
            info="Request timeout in seconds",
            value="30.0",
            required=False,
        ),
    ]

    outputs = [
        Output(
            display_name="Client",
            name="client",
            method="build_client",
        ),
        Output(
            display_name="Connection Status",
            name="status",
            method="check_connection",
        ),
    ]

    def build_client(self) -> NexusClient:
        """Build and return a NexusClient instance."""
        timeout = float(self.timeout) if self.timeout else 30.0

        client = NexusClient(
            url=self.url,
            api_key=self.api_key if self.api_key else None,
            username=self.username if self.username else None,
            password=self.password if self.password else None,
            timeout=timeout,
        )

        return client

    def check_connection(self) -> Data:
        """Check connection to the Nexus server."""
        try:
            client = self.build_client()
            is_healthy = client.health_check_sync()

            if is_healthy:
                return Data(
                    data={
                        "status": "connected",
                        "url": self.url,
                        "message": "Successfully connected to Nexus server",
                    }
                )
            else:
                return Data(
                    data={
                        "status": "unhealthy",
                        "url": self.url,
                        "message": "Server responded but health check failed",
                    }
                )
        except Exception as e:
            return Data(
                data={
                    "status": "error",
                    "url": self.url,
                    "message": f"Connection failed: {str(e)}",
                }
            )
