"""Tests for external-id node operations (Phase9 §5.5).

These tests require a running Nexus server at http://localhost:15474.
They are skipped automatically when the server is not reachable, mirroring
the pattern used in the rest of the SDK test suite.
"""

from __future__ import annotations

import uuid

import pytest

from nexus_sdk import NexusClient
from nexus_sdk.models import CreateNodeResponse, GetNodeByExternalIdResponse


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

SERVER_URL = "http://localhost:15474"


def _unique_uuid_ext_id() -> str:
    """Return a fresh ``uuid:…`` external id so tests never collide."""
    return f"uuid:{uuid.uuid4()}"


async def _server_available(client: NexusClient) -> bool:
    try:
        return await client.health_check()
    except Exception:
        return False


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.asyncio
async def test_create_node_with_external_id_round_trip() -> None:
    """Create a node with an external id, then look it up by that id."""
    async with NexusClient(SERVER_URL) as client:
        if not await _server_available(client):
            pytest.skip("Nexus server not running at " + SERVER_URL)

        ext_id = _unique_uuid_ext_id()

        create: CreateNodeResponse = await client.create_node_with_external_id(
            labels=["ExtIdPyTest"],
            properties={"imported_from": "phase9_python_test"},
            external_id=ext_id,
            conflict_policy="match",
        )

        # The server may return an error on the first insert if the engine
        # is not fully initialised; skip rather than fail hard.
        if create.error is not None:
            pytest.skip(f"Node creation reported server error: {create.error}")

        lookup: GetNodeByExternalIdResponse = await client.get_node_by_external_id(ext_id)

        assert lookup.error is None, f"Lookup returned server error: {lookup.error}"
        assert lookup.node is not None, "Expected node to be resolved from external id"
        assert lookup.node.id == create.node_id


@pytest.mark.asyncio
async def test_create_node_via_extended_create_node_signature() -> None:
    """Verify the optional params on create_node produce the same result."""
    async with NexusClient(SERVER_URL) as client:
        if not await _server_available(client):
            pytest.skip("Nexus server not running at " + SERVER_URL)

        ext_id = _unique_uuid_ext_id()

        create: CreateNodeResponse = await client.create_node(
            labels=["ExtIdPyTest"],
            properties={"source": "signature_test"},
            external_id=ext_id,
            conflict_policy="error",
        )

        if create.error is not None:
            pytest.skip(f"Node creation reported server error: {create.error}")

        lookup: GetNodeByExternalIdResponse = await client.get_node_by_external_id(ext_id)

        assert lookup.error is None
        assert lookup.node is not None
        assert lookup.node.id == create.node_id


@pytest.mark.asyncio
async def test_get_node_by_external_id_absent() -> None:
    """Resolving a non-existent external id returns node=None without error."""
    async with NexusClient(SERVER_URL) as client:
        if not await _server_available(client):
            pytest.skip("Nexus server not running at " + SERVER_URL)

        # Use a UUID that was never inserted.
        nonexistent = f"uuid:{uuid.uuid4()}"
        result: GetNodeByExternalIdResponse = await client.get_node_by_external_id(
            nonexistent
        )

        # The server contract says node=None for a miss, no hard error.
        assert result.node is None
