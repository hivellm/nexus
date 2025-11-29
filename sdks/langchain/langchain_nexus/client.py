"""Nexus HTTP client for LangChain integration."""

import asyncio
import base64
from typing import Any, Dict, List, Optional, Tuple
from urllib.parse import urljoin

import httpx


class NexusClient:
    """HTTP client for Nexus graph database.

    This client provides low-level access to Nexus APIs for use by
    LangChain components.

    Args:
        url: Base URL of the Nexus server (e.g., "http://localhost:15474")
        api_key: Optional API key for authentication
        username: Optional username for basic authentication
        password: Optional password for basic authentication
        timeout: Request timeout in seconds (default: 30.0)

    Example:
        >>> client = NexusClient("http://localhost:15474", api_key="my-key")
        >>> result = await client.execute_cypher("MATCH (n) RETURN count(n)")
    """

    def __init__(
        self,
        url: str,
        api_key: Optional[str] = None,
        username: Optional[str] = None,
        password: Optional[str] = None,
        timeout: float = 30.0,
    ):
        self.url = url.rstrip("/")
        self.api_key = api_key
        self.username = username
        self.password = password
        self.timeout = timeout
        self._client: Optional[httpx.AsyncClient] = None
        self._sync_client: Optional[httpx.Client] = None

    def _get_auth_headers(self) -> Dict[str, str]:
        """Get authentication headers."""
        headers = {"Content-Type": "application/json"}
        if self.api_key:
            headers["X-API-Key"] = self.api_key
        elif self.username and self.password:
            auth_string = f"{self.username}:{self.password}"
            auth_bytes = auth_string.encode("utf-8")
            auth_b64 = base64.b64encode(auth_bytes).decode("utf-8")
            headers["Authorization"] = f"Basic {auth_b64}"
        return headers

    async def _get_async_client(self) -> httpx.AsyncClient:
        """Get or create async HTTP client."""
        if self._client is None:
            self._client = httpx.AsyncClient(
                timeout=httpx.Timeout(self.timeout),
                headers=self._get_auth_headers(),
            )
        return self._client

    def _get_sync_client(self) -> httpx.Client:
        """Get or create sync HTTP client."""
        if self._sync_client is None:
            self._sync_client = httpx.Client(
                timeout=httpx.Timeout(self.timeout),
                headers=self._get_auth_headers(),
            )
        return self._sync_client

    async def close(self) -> None:
        """Close the HTTP clients."""
        if self._client:
            await self._client.aclose()
            self._client = None
        if self._sync_client:
            self._sync_client.close()
            self._sync_client = None

    async def execute_cypher(
        self,
        query: str,
        parameters: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Execute a Cypher query asynchronously.

        Args:
            query: Cypher query string
            parameters: Optional query parameters

        Returns:
            Query result with columns, rows, and stats

        Raises:
            httpx.HTTPStatusError: If the request fails
        """
        client = await self._get_async_client()
        url = urljoin(self.url, "/cypher")
        payload = {"query": query, "parameters": parameters or {}}

        response = await client.post(url, json=payload)
        response.raise_for_status()
        return response.json()

    def execute_cypher_sync(
        self,
        query: str,
        parameters: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Execute a Cypher query synchronously.

        Args:
            query: Cypher query string
            parameters: Optional query parameters

        Returns:
            Query result with columns, rows, and stats
        """
        client = self._get_sync_client()
        url = urljoin(self.url, "/cypher")
        payload = {"query": query, "parameters": parameters or {}}

        response = client.post(url, json=payload)
        response.raise_for_status()
        return response.json()

    async def knn_search(
        self,
        label: str,
        vector: List[float],
        k: int = 10,
        property_name: str = "embedding",
    ) -> List[Dict[str, Any]]:
        """Perform KNN vector search.

        Args:
            label: Node label to search
            vector: Query vector
            k: Number of results to return
            property_name: Name of the vector property

        Returns:
            List of matching nodes with scores
        """
        client = await self._get_async_client()
        url = urljoin(self.url, "/knn_traverse")
        payload = {
            "label": label,
            "vector": vector,
            "k": k,
            "property_name": property_name,
        }

        response = await client.post(url, json=payload)
        response.raise_for_status()
        return response.json().get("results", [])

    def knn_search_sync(
        self,
        label: str,
        vector: List[float],
        k: int = 10,
        property_name: str = "embedding",
    ) -> List[Dict[str, Any]]:
        """Perform KNN vector search synchronously.

        Args:
            label: Node label to search
            vector: Query vector
            k: Number of results to return
            property_name: Name of the vector property

        Returns:
            List of matching nodes with scores
        """
        client = self._get_sync_client()
        url = urljoin(self.url, "/knn_traverse")
        payload = {
            "label": label,
            "vector": vector,
            "k": k,
            "property_name": property_name,
        }

        response = client.post(url, json=payload)
        response.raise_for_status()
        return response.json().get("results", [])

    async def create_node(
        self,
        labels: List[str],
        properties: Dict[str, Any],
    ) -> int:
        """Create a node.

        Args:
            labels: Node labels
            properties: Node properties

        Returns:
            Created node ID
        """
        client = await self._get_async_client()
        url = urljoin(self.url, "/data/nodes")
        payload = {"labels": labels, "properties": properties}

        response = await client.post(url, json=payload)
        response.raise_for_status()
        return response.json().get("node_id", 0)

    def create_node_sync(
        self,
        labels: List[str],
        properties: Dict[str, Any],
    ) -> int:
        """Create a node synchronously.

        Args:
            labels: Node labels
            properties: Node properties

        Returns:
            Created node ID
        """
        client = self._get_sync_client()
        url = urljoin(self.url, "/data/nodes")
        payload = {"labels": labels, "properties": properties}

        response = client.post(url, json=payload)
        response.raise_for_status()
        return response.json().get("node_id", 0)

    async def create_relationship(
        self,
        source_id: int,
        target_id: int,
        rel_type: str,
        properties: Optional[Dict[str, Any]] = None,
    ) -> int:
        """Create a relationship between nodes.

        Args:
            source_id: Source node ID
            target_id: Target node ID
            rel_type: Relationship type
            properties: Optional relationship properties

        Returns:
            Created relationship ID
        """
        client = await self._get_async_client()
        url = urljoin(self.url, "/data/relationships")
        payload = {
            "source_id": source_id,
            "target_id": target_id,
            "rel_type": rel_type,
            "properties": properties or {},
        }

        response = await client.post(url, json=payload)
        response.raise_for_status()
        return response.json().get("relationship_id", 0)

    async def health_check(self) -> bool:
        """Check if the server is healthy.

        Returns:
            True if healthy, False otherwise
        """
        try:
            client = await self._get_async_client()
            url = urljoin(self.url, "/health")
            response = await client.get(url)
            return response.status_code == 200
        except Exception:
            return False

    def health_check_sync(self) -> bool:
        """Check if the server is healthy synchronously.

        Returns:
            True if healthy, False otherwise
        """
        try:
            client = self._get_sync_client()
            url = urljoin(self.url, "/health")
            response = client.get(url)
            return response.status_code == 200
        except Exception:
            return False
