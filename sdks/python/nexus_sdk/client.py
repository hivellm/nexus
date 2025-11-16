"""Nexus client implementation."""

import asyncio
import base64
from typing import Any, Dict, List, Optional
import httpx
from urllib.parse import urljoin

from nexus_sdk.error import (
    ApiError,
    AuthenticationError,
    ConfigurationError,
    HttpError,
    NetworkError,
    TimeoutError,
)
from nexus_sdk.models import (
    QueryResult,
    DatabaseStats,
    Node,
    Relationship,
    CreateNodeRequest,
    CreateNodeResponse,
    UpdateNodeRequest,
    UpdateNodeResponse,
    DeleteNodeRequest,
    DeleteNodeResponse,
    CreateRelationshipRequest,
    CreateRelationshipResponse,
    UpdateRelationshipRequest,
    UpdateRelationshipResponse,
    DeleteRelationshipRequest,
    DeleteRelationshipResponse,
    LabelResponse,
    RelTypeResponse,
    TransactionResponse,
)


class NexusClient:
    """Nexus client for interacting with the Nexus graph database."""

    def __init__(
        self,
        base_url: str,
        api_key: Optional[str] = None,
        username: Optional[str] = None,
        password: Optional[str] = None,
        timeout: float = 30.0,
        max_retries: int = 3,
    ):
        """Create a new Nexus client.

        Args:
            base_url: Base URL of the Nexus server (e.g., "http://localhost:15474")
            api_key: Optional API key for authentication
            username: Optional username for authentication
            password: Optional password for authentication
            timeout: Request timeout in seconds (default: 30.0)
            max_retries: Maximum number of retries for failed requests (default: 3)

        Raises:
            ConfigurationError: If the base URL is invalid
        """
        try:
            self.base_url = base_url.rstrip("/")
            self.api_key = api_key
            self.username = username
            self.password = password
            self.timeout = timeout
            self.max_retries = max_retries

            self._client = httpx.AsyncClient(
                timeout=httpx.Timeout(timeout),
                headers={"User-Agent": "nexus-sdk/0.1.0"},
            )
        except Exception as e:
            raise ConfigurationError(f"Invalid configuration: {e}") from e

    async def __aenter__(self):
        """Async context manager entry."""
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.close()

    async def close(self):
        """Close the HTTP client."""
        await self._client.aclose()

    def _get_auth_headers(self) -> Dict[str, str]:
        """Get authentication headers."""
        headers = {}
        if self.api_key:
            headers["X-API-Key"] = self.api_key
        elif self.username and self.password:
            auth_string = f"{self.username}:{self.password}"
            auth_bytes = auth_string.encode("utf-8")
            auth_b64 = base64.b64encode(auth_bytes).decode("utf-8")
            headers["Authorization"] = f"Basic {auth_b64}"
        return headers

    async def _execute_with_retry(
        self, method: str, url: str, **kwargs
    ) -> httpx.Response:
        """Execute HTTP request with retry logic."""
        headers = self._get_auth_headers()
        if "headers" in kwargs:
            headers.update(kwargs["headers"])
        kwargs["headers"] = headers

        last_error = None
        for attempt in range(self.max_retries + 1):
            try:
                response = await self._client.request(method, url, **kwargs)
                status = response.status_code

                # Check if status is retryable (5xx errors)
                if status >= 500 and attempt < self.max_retries:
                    delay_ms = 100 * (1 << min(attempt, 5))  # Cap at 3.2s
                    await asyncio.sleep(delay_ms / 1000.0)
                    continue

                return response
            except httpx.TimeoutException as e:
                last_error = e
                if attempt < self.max_retries:
                    delay_ms = 100 * (1 << min(attempt, 5))
                    await asyncio.sleep(delay_ms / 1000.0)
                    continue
                raise TimeoutError("Request timeout") from e
            except httpx.NetworkError as e:
                last_error = e
                if attempt < self.max_retries:
                    delay_ms = 100 * (1 << min(attempt, 5))
                    await asyncio.sleep(delay_ms / 1000.0)
                    continue
                raise NetworkError(f"Network error: {e}") from e
            except httpx.HTTPError as e:
                last_error = e
                if attempt < self.max_retries:
                    delay_ms = 100 * (1 << min(attempt, 5))
                    await asyncio.sleep(delay_ms / 1000.0)
                    continue
                raise HttpError(f"HTTP error: {e}", status_code=None) from e

        if last_error:
            raise HttpError(f"Request failed after retries: {last_error}") from last_error
        raise NetworkError("Request failed after retries")

    async def execute_cypher(
        self, query: str, parameters: Optional[Dict[str, Any]] = None
    ) -> QueryResult:
        """Execute a Cypher query.

        Args:
            query: Cypher query string
            parameters: Optional query parameters

        Returns:
            QueryResult containing columns, rows, and execution metadata

        Raises:
            ApiError: If the API returns an error
            HttpError: If there's an HTTP error
        """
        url = urljoin(self.base_url, "/cypher")
        payload = {"query": query, "parameters": parameters or {}}

        response = await self._execute_with_retry("POST", url, json=payload)
        status = response.status_code

        if status == 200:
            data = response.json()
            return QueryResult(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def get_stats(self) -> DatabaseStats:
        """Get database statistics.

        Returns:
            DatabaseStats containing catalog and storage information

        Raises:
            ApiError: If the API returns an error
        """
        url = urljoin(self.base_url, "/stats")
        response = await self._execute_with_retry("GET", url)
        status = response.status_code

        if status == 200:
            data = response.json()
            return DatabaseStats(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def health_check(self) -> bool:
        """Check server health.

        Returns:
            True if server is healthy, False otherwise
        """
        try:
            url = urljoin(self.base_url, "/health")
            response = await self._execute_with_retry("GET", url)
            return response.status_code == 200
        except Exception:
            return False

    async def create_node(
        self, labels: List[str], properties: Dict[str, Any]
    ) -> CreateNodeResponse:
        """Create a new node.

        Args:
            labels: Node labels
            properties: Node properties

        Returns:
            CreateNodeResponse containing the created node ID

        Raises:
            ApiError: If the API returns an error
        """
        url = urljoin(self.base_url, "/data/nodes")
        payload = CreateNodeRequest(labels=labels, properties=properties).model_dump()

        response = await self._execute_with_retry("POST", url, json=payload)
        status = response.status_code

        if status == 200:
            data = response.json()
            return CreateNodeResponse(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def get_node(self, node_id: int) -> Optional[Node]:
        """Get a node by ID.

        Args:
            node_id: Node ID

        Returns:
            Node if found, None otherwise

        Raises:
            ApiError: If the API returns an error
        """
        url = urljoin(self.base_url, f"/data/nodes/{node_id}")
        response = await self._execute_with_retry("GET", url)
        status = response.status_code

        if status == 200:
            data = response.json()
            if "node" in data and data["node"]:
                return Node(**data["node"])
            return None
        elif status == 404:
            return None
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def update_node(
        self,
        node_id: int,
        labels: Optional[List[str]] = None,
        properties: Optional[Dict[str, Any]] = None,
    ) -> UpdateNodeResponse:
        """Update a node.

        Args:
            node_id: Node ID
            labels: Optional new labels (will replace existing)
            properties: Optional new properties (will replace existing)

        Returns:
            UpdateNodeResponse containing the updated node

        Raises:
            ApiError: If the API returns an error
        """
        url = urljoin(self.base_url, f"/data/nodes/{node_id}")
        payload = UpdateNodeRequest(
            node_id=node_id, labels=labels, properties=properties
        ).model_dump(exclude_none=True)

        response = await self._execute_with_retry("PUT", url, json=payload)
        status = response.status_code

        if status == 200:
            data = response.json()
            return UpdateNodeResponse(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def delete_node(self, node_id: int) -> DeleteNodeResponse:
        """Delete a node.

        Args:
            node_id: Node ID

        Returns:
            DeleteNodeResponse indicating success or failure

        Raises:
            ApiError: If the API returns an error
        """
        url = urljoin(self.base_url, f"/data/nodes/{node_id}")
        response = await self._execute_with_retry("DELETE", url)
        status = response.status_code

        if status == 200:
            data = response.json()
            return DeleteNodeResponse(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def create_relationship(
        self,
        source_id: int,
        target_id: int,
        rel_type: str,
        properties: Optional[Dict[str, Any]] = None,
    ) -> CreateRelationshipResponse:
        """Create a new relationship.

        Args:
            source_id: Source node ID
            target_id: Target node ID
            rel_type: Relationship type
            properties: Optional relationship properties

        Returns:
            CreateRelationshipResponse containing the created relationship ID

        Raises:
            ApiError: If the API returns an error
        """
        url = urljoin(self.base_url, "/data/relationships")
        payload = CreateRelationshipRequest(
            source_id=source_id,
            target_id=target_id,
            rel_type=rel_type,
            properties=properties or {},
        ).model_dump()

        response = await self._execute_with_retry("POST", url, json=payload)
        status = response.status_code

        if status == 200:
            data = response.json()
            return CreateRelationshipResponse(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def update_relationship(
        self, relationship_id: int, properties: Dict[str, Any]
    ) -> UpdateRelationshipResponse:
        """Update a relationship.

        Args:
            relationship_id: Relationship ID
            properties: New properties (will replace existing)

        Returns:
            UpdateRelationshipResponse containing the updated relationship

        Raises:
            ApiError: If the API returns an error
        """
        url = urljoin(self.base_url, f"/data/relationships/{relationship_id}")
        payload = UpdateRelationshipRequest(
            relationship_id=relationship_id, properties=properties
        ).model_dump()

        response = await self._execute_with_retry("PUT", url, json=payload)
        status = response.status_code

        if status == 200:
            data = response.json()
            return UpdateRelationshipResponse(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def delete_relationship(
        self, relationship_id: int
    ) -> DeleteRelationshipResponse:
        """Delete a relationship.

        Args:
            relationship_id: Relationship ID

        Returns:
            DeleteRelationshipResponse indicating success or failure

        Raises:
            ApiError: If the API returns an error
        """
        url = urljoin(self.base_url, f"/data/relationships/{relationship_id}")
        response = await self._execute_with_retry("DELETE", url)
        status = response.status_code

        if status == 200:
            data = response.json()
            return DeleteRelationshipResponse(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def create_label(self, name: str) -> LabelResponse:
        """Create a new label.

        Args:
            name: Label name

        Returns:
            LabelResponse indicating success or failure

        Raises:
            ApiError: If the API returns an error
        """
        url = urljoin(self.base_url, "/schema/labels")
        payload = {"name": name}

        response = await self._execute_with_retry("POST", url, json=payload)
        status = response.status_code

        if status == 200:
            data = response.json()
            return LabelResponse(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def list_labels(self) -> LabelResponse:
        """List all labels.

        Returns:
            LabelResponse containing list of labels

        Raises:
            ApiError: If the API returns an error
        """
        url = urljoin(self.base_url, "/schema/labels")
        response = await self._execute_with_retry("GET", url)
        status = response.status_code

        if status == 200:
            data = response.json()
            # Convert list of tuples to list of strings
            if "labels" in data and isinstance(data["labels"], list):
                labels = [label[0] if isinstance(label, (list, tuple)) else label for label in data["labels"]]
                data["labels"] = labels
            return LabelResponse(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def create_rel_type(self, name: str) -> RelTypeResponse:
        """Create a new relationship type.

        Args:
            name: Relationship type name

        Returns:
            RelTypeResponse indicating success or failure

        Raises:
            ApiError: If the API returns an error
        """
        url = urljoin(self.base_url, "/schema/rel-types")
        payload = {"name": name}

        response = await self._execute_with_retry("POST", url, json=payload)
        status = response.status_code

        if status == 200:
            data = response.json()
            return RelTypeResponse(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def list_rel_types(self) -> RelTypeResponse:
        """List all relationship types.

        Returns:
            RelTypeResponse containing list of relationship types

        Raises:
            ApiError: If the API returns an error
        """
        url = urljoin(self.base_url, "/schema/rel-types")
        response = await self._execute_with_retry("GET", url)
        status = response.status_code

        if status == 200:
            data = response.json()
            # Convert list of tuples to list of strings
            if "types" in data and isinstance(data["types"], list):
                types = [t[0] if isinstance(t, (list, tuple)) else t for t in data["types"]]
                data["types"] = types
            return RelTypeResponse(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def begin_transaction(self) -> "Transaction":
        """Begin a new transaction.

        Returns:
            Transaction object for managing the transaction

        Raises:
            ApiError: If the API returns an error
        """
        from nexus_sdk.transaction import Transaction

        tx = Transaction(self)
        await tx.begin()
        return tx

    async def begin_transaction_simple(self) -> TransactionResponse:
        """Begin a new transaction (simple version, returns response).

        Returns:
            TransactionResponse containing transaction ID

        Raises:
            ApiError: If the API returns an error
        """
        result = await self.execute_cypher("BEGIN TRANSACTION", None)
        return TransactionResponse(
            transaction_id=f"tx_{asyncio.get_event_loop().time()}",
            success=True,
        )

    async def commit_transaction(self) -> TransactionResponse:
        """Commit the current transaction.

        Returns:
            TransactionResponse indicating success or failure

        Raises:
            ApiError: If the API returns an error
        """
        result = await self.execute_cypher("COMMIT TRANSACTION", None)
        return TransactionResponse(success=True)

    async def rollback_transaction(self) -> TransactionResponse:
        """Rollback the current transaction.

        Returns:
            TransactionResponse indicating success or failure

        Raises:
            ApiError: If the API returns an error
        """
        result = await self.execute_cypher("ROLLBACK TRANSACTION", None)
        return TransactionResponse(success=True)

    async def batch_create_nodes(
        self, nodes: List[Dict[str, Any]]
    ) -> "BatchCreateNodesResponse":
        """Batch create multiple nodes.

        Args:
            nodes: List of node definitions, each with 'labels' and 'properties'

        Returns:
            BatchCreateNodesResponse containing list of created node IDs

        Raises:
            ApiError: If the API returns an error
        """
        from nexus_sdk.models import BatchNode, BatchCreateNodesResponse

        # For now, create nodes sequentially
        # TODO: Implement proper batch endpoint if available
        node_ids = []
        errors = []

        for node_data in nodes:
            try:
                batch_node = BatchNode(**node_data)
                response = await self.create_node(
                    batch_node.labels, batch_node.properties
                )
                if response.node_id > 0:
                    node_ids.append(response.node_id)
            except Exception as e:
                errors.append(f"Failed to create node: {e}")

        if errors:
            from nexus_sdk.error import ValidationError
            raise ValidationError(f"Some nodes failed to create: {', '.join(errors)}")

        return BatchCreateNodesResponse(
            node_ids=node_ids,
            message=f"Successfully created {len(node_ids)} nodes",
        )

    async def batch_create_relationships(
        self, relationships: List[Dict[str, Any]]
    ) -> "BatchCreateRelationshipsResponse":
        """Batch create multiple relationships.

        Args:
            relationships: List of relationship definitions, each with 'source_id', 'target_id', 'rel_type', and 'properties'

        Returns:
            BatchCreateRelationshipsResponse containing list of created relationship IDs

        Raises:
            ApiError: If the API returns an error
        """
        from nexus_sdk.models import (
            BatchRelationship,
            BatchCreateRelationshipsResponse,
        )

        # For now, create relationships sequentially
        # TODO: Implement proper batch endpoint if available
        rel_ids = []
        errors = []

        for rel_data in relationships:
            try:
                batch_rel = BatchRelationship(**rel_data)
                response = await self.create_relationship(
                    batch_rel.source_id,
                    batch_rel.target_id,
                    batch_rel.rel_type,
                    batch_rel.properties,
                )
                rel_ids.append(response.relationship_id)
            except Exception as e:
                errors.append(f"Failed to create relationship: {e}")

        if errors:
            from nexus_sdk.error import ValidationError
            raise ValidationError(
                f"Some relationships failed to create: {', '.join(errors)}"
            )

        return BatchCreateRelationshipsResponse(
            rel_ids=rel_ids,
            message=f"Successfully created {len(rel_ids)} relationships",
        )

    async def get_query_statistics(self) -> "QueryStatisticsResponse":
        """Get query statistics.

        Returns:
            QueryStatisticsResponse containing query statistics

        Raises:
            ApiError: If the API returns an error
        """
        from nexus_sdk.models import QueryStatisticsResponse

        url = urljoin(self.base_url, "/performance/statistics")
        response = await self._execute_with_retry("GET", url)
        status = response.status_code

        if status == 200:
            data = response.json()
            return QueryStatisticsResponse(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def get_slow_queries(self) -> "SlowQueriesResponse":
        """Get slow queries.

        Returns:
            SlowQueriesResponse containing slow query records

        Raises:
            ApiError: If the API returns an error
        """
        from nexus_sdk.models import SlowQueriesResponse

        url = urljoin(self.base_url, "/performance/slow-queries")
        response = await self._execute_with_retry("GET", url)
        status = response.status_code

        if status == 200:
            data = response.json()
            return SlowQueriesResponse(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def get_plan_cache_statistics(self) -> "PlanCacheStatisticsResponse":
        """Get plan cache statistics.

        Returns:
            PlanCacheStatisticsResponse containing plan cache statistics

        Raises:
            ApiError: If the API returns an error
        """
        from nexus_sdk.models import PlanCacheStatisticsResponse

        url = urljoin(self.base_url, "/performance/plan-cache")
        response = await self._execute_with_retry("GET", url)
        status = response.status_code

        if status == 200:
            data = response.json()
            return PlanCacheStatisticsResponse(**data)
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)

    async def clear_plan_cache(self) -> Dict[str, Any]:
        """Clear plan cache.

        Returns:
            Response indicating success or failure

        Raises:
            ApiError: If the API returns an error
        """
        url = urljoin(self.base_url, "/performance/plan-cache/clear")
        response = await self._execute_with_retry("POST", url)
        status = response.status_code

        if status == 200:
            return response.json()
        else:
            try:
                error_text = response.text
            except Exception:
                error_text = f"HTTP {status}"
            raise ApiError(error_text, status)
