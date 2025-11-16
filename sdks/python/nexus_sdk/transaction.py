"""Transaction support for Nexus SDK."""

from typing import Any, Dict, Optional
from enum import Enum

from nexus_sdk.client import NexusClient
from nexus_sdk.error import ValidationError
from nexus_sdk.models import QueryResult, Value


class TransactionStatus(Enum):
    """Transaction status."""

    ACTIVE = "active"
    COMMITTED = "committed"
    ROLLED_BACK = "rolled_back"
    NOT_STARTED = "not_started"


class Transaction:
    """Transaction handle for managing database transactions."""

    def __init__(self, client: NexusClient):
        """Create a new transaction handle.

        Args:
            client: NexusClient instance
        """
        self._client = client
        self._transaction_id: Optional[str] = None
        self._active: bool = False

    async def begin(self) -> None:
        """Begin a new transaction.

        Raises:
            ValidationError: If transaction is already active
            ApiError: If the API returns an error
        """
        if self._active:
            raise ValidationError("Transaction already active")

        await self._client.execute_cypher("BEGIN TRANSACTION", None)

        import asyncio

        self._active = True
        self._transaction_id = f"tx_{asyncio.get_event_loop().time()}"

    async def commit(self) -> None:
        """Commit the transaction.

        Raises:
            ValidationError: If no active transaction to commit
            ApiError: If the API returns an error
        """
        if not self._active:
            raise ValidationError("No active transaction to commit")

        await self._client.execute_cypher("COMMIT TRANSACTION", None)

        self._active = False
        self._transaction_id = None

    async def rollback(self) -> None:
        """Rollback the transaction.

        Raises:
            ValidationError: If no active transaction to rollback
            ApiError: If the API returns an error
        """
        if not self._active:
            raise ValidationError("No active transaction to rollback")

        await self._client.execute_cypher("ROLLBACK TRANSACTION", None)

        self._active = False
        self._transaction_id = None

    def is_active(self) -> bool:
        """Check if transaction is active.

        Returns:
            True if transaction is active, False otherwise
        """
        return self._active

    def status(self) -> TransactionStatus:
        """Get transaction status.

        Returns:
            TransactionStatus enum value
        """
        if self._active:
            return TransactionStatus.ACTIVE
        elif self._transaction_id is not None:
            return TransactionStatus.COMMITTED
        else:
            return TransactionStatus.NOT_STARTED

    async def execute(
        self, query: str, parameters: Optional[Dict[str, Value]] = None
    ) -> QueryResult:
        """Execute a Cypher query within this transaction.

        Args:
            query: Cypher query string
            parameters: Optional query parameters

        Returns:
            QueryResult containing query results

        Raises:
            ValidationError: If transaction is not active
            ApiError: If the API returns an error
        """
        if not self._active:
            raise ValidationError("Transaction is not active")

        return await self._client.execute_cypher(query, parameters)

    @property
    def transaction_id(self) -> Optional[str]:
        """Get the transaction ID.

        Returns:
            Transaction ID or None
        """
        return self._transaction_id

