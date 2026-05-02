"""Live integration tests for external-id node operations (Phase10 §2).

Gate: set the ``NEXUS_LIVE_HOST`` environment variable to the server URL
(e.g. ``http://localhost:15474``) to run these tests against a real server.

Without the variable every test is auto-skipped so unit-only CI passes
without a running container.

Run live:
    NEXUS_LIVE_HOST=http://localhost:15474 pytest \
        sdks/python/nexus_sdk/tests/test_external_id_live.py -v -m live
"""

from __future__ import annotations

import os
import uuid

import pytest

from nexus_sdk import NexusClient
from nexus_sdk.models import CreateNodeResponse, GetNodeByExternalIdResponse

# ---------------------------------------------------------------------------
# Infrastructure
# ---------------------------------------------------------------------------

LIVE_HOST = os.environ.get("NEXUS_LIVE_HOST")
pytestmark = pytest.mark.skipif(
    LIVE_HOST is None,
    reason="set NEXUS_LIVE_HOST to run live tests",
)


def _pfx() -> str:
    """Short unique test-run prefix so each test uses collision-free ext ids."""
    return uuid.uuid4().hex[:12]


# ---------------------------------------------------------------------------
# 2.2 — All six ExternalId variant round-trips
# ---------------------------------------------------------------------------


@pytest.mark.live
@pytest.mark.asyncio
async def test_sha256_variant_round_trip() -> None:
    """should resolve the correct node when created with a sha256 external id."""
    async with NexusClient(LIVE_HOST) as client:
        p = _pfx()
        ext = f"sha256:{p}{'0' * (64 - len(p))}"
        create: CreateNodeResponse = await client.create_node_with_external_id(
            labels=["LiveTestSha256"],
            properties={"variant": "sha256", "pfx": p},
            external_id=ext,
        )
        assert create.error is None, f"Create error: {create.error}"
        assert create.node_id > 0

        lookup: GetNodeByExternalIdResponse = await client.get_node_by_external_id(ext)
        assert lookup.node is not None, "Expected node, got None"
        assert lookup.node.id == create.node_id


@pytest.mark.live
@pytest.mark.asyncio
async def test_blake3_variant_round_trip() -> None:
    """should resolve the correct node when created with a blake3 external id."""
    async with NexusClient(LIVE_HOST) as client:
        p = _pfx()
        ext = f"blake3:{p}{'1' * (64 - len(p))}"
        create: CreateNodeResponse = await client.create_node_with_external_id(
            labels=["LiveTestBlake3"],
            properties={"variant": "blake3", "pfx": p},
            external_id=ext,
        )
        assert create.error is None, f"Create error: {create.error}"
        assert create.node_id > 0

        lookup: GetNodeByExternalIdResponse = await client.get_node_by_external_id(ext)
        assert lookup.node is not None, "Expected node, got None"
        assert lookup.node.id == create.node_id


@pytest.mark.live
@pytest.mark.asyncio
async def test_sha512_variant_round_trip() -> None:
    """should resolve the correct node when created with a sha512 external id."""
    async with NexusClient(LIVE_HOST) as client:
        p = _pfx()
        ext = f"sha512:{p}{'2' * (128 - len(p))}"
        create: CreateNodeResponse = await client.create_node_with_external_id(
            labels=["LiveTestSha512"],
            properties={"variant": "sha512", "pfx": p},
            external_id=ext,
        )
        assert create.error is None, f"Create error: {create.error}"
        assert create.node_id > 0

        lookup: GetNodeByExternalIdResponse = await client.get_node_by_external_id(ext)
        assert lookup.node is not None, "Expected node, got None"
        assert lookup.node.id == create.node_id


@pytest.mark.live
@pytest.mark.asyncio
async def test_uuid_variant_round_trip() -> None:
    """should resolve the correct node when created with a uuid external id."""
    async with NexusClient(LIVE_HOST) as client:
        ext = f"uuid:{uuid.uuid4()}"
        create: CreateNodeResponse = await client.create_node_with_external_id(
            labels=["LiveTestUuid"],
            properties={"variant": "uuid"},
            external_id=ext,
        )
        assert create.error is None, f"Create error: {create.error}"
        assert create.node_id > 0

        lookup: GetNodeByExternalIdResponse = await client.get_node_by_external_id(ext)
        assert lookup.node is not None, "Expected node, got None"
        assert lookup.node.id == create.node_id


@pytest.mark.live
@pytest.mark.asyncio
async def test_str_variant_round_trip() -> None:
    """should resolve the correct node when created with a str external id."""
    async with NexusClient(LIVE_HOST) as client:
        p = _pfx()
        ext = f"str:live-test-{p}"
        create: CreateNodeResponse = await client.create_node_with_external_id(
            labels=["LiveTestStr"],
            properties={"variant": "str", "key": p},
            external_id=ext,
        )
        assert create.error is None, f"Create error: {create.error}"
        assert create.node_id > 0

        lookup: GetNodeByExternalIdResponse = await client.get_node_by_external_id(ext)
        assert lookup.node is not None, "Expected node, got None"
        assert lookup.node.id == create.node_id


@pytest.mark.live
@pytest.mark.asyncio
async def test_bytes_variant_round_trip() -> None:
    """should resolve the correct node when created with a bytes external id."""
    async with NexusClient(LIVE_HOST) as client:
        # 4 bytes (8 hex chars) is well inside the 64-byte cap.
        p = _pfx()[:8]
        ext = f"bytes:{p}"
        create: CreateNodeResponse = await client.create_node_with_external_id(
            labels=["LiveTestBytes"],
            properties={"variant": "bytes", "pfx": p},
            external_id=ext,
        )
        assert create.error is None, f"Create error: {create.error}"
        assert create.node_id > 0

        lookup: GetNodeByExternalIdResponse = await client.get_node_by_external_id(ext)
        assert lookup.node is not None, "Expected node, got None"
        assert lookup.node.id == create.node_id


# ---------------------------------------------------------------------------
# 2.3 — Three conflict policies; REPLACE must overwrite a property
# ---------------------------------------------------------------------------


@pytest.mark.live
@pytest.mark.asyncio
async def test_conflict_policy_error_rejects_duplicate() -> None:
    """should populate response.error when conflict_policy=error and external id already exists.

    The Nexus server responds with HTTP 200 but sets the ``error`` field in
    the JSON body when a duplicate is detected (rather than a 4xx status).
    The SDK surfaces this as a populated ``CreateNodeResponse.error`` string.
    """
    async with NexusClient(LIVE_HOST) as client:
        ext = f"uuid:{uuid.uuid4()}"
        first = await client.create_node_with_external_id(
            labels=["LiveTestConflict"],
            properties={"pass": "first"},
            external_id=ext,
            conflict_policy="error",
        )
        assert first.error is None, f"First create should succeed, got: {first.error}"

        second = await client.create_node_with_external_id(
            labels=["LiveTestConflict"],
            properties={"pass": "second"},
            external_id=ext,
            conflict_policy="error",
        )
        # Server signals conflict via the body error field (HTTP 200).
        assert (
            second.error is not None
        ), "Expected an error on duplicate with conflict_policy=error"


@pytest.mark.live
@pytest.mark.asyncio
async def test_conflict_policy_match_returns_existing_id() -> None:
    """should return the existing node_id when conflict_policy=match and id is already present."""
    async with NexusClient(LIVE_HOST) as client:
        ext = f"uuid:{uuid.uuid4()}"
        first = await client.create_node_with_external_id(
            labels=["LiveTestMatch"],
            properties={"val": "original"},
            external_id=ext,
        )
        assert first.error is None

        second = await client.create_node_with_external_id(
            labels=["LiveTestMatch"],
            properties={"val": "ignored"},
            external_id=ext,
            conflict_policy="match",
        )
        assert second.error is None
        assert second.node_id == first.node_id


@pytest.mark.live
@pytest.mark.asyncio
async def test_conflict_policy_replace_overwrites_properties() -> None:
    """should overwrite property values after conflict_policy=replace — regression guard for fd001344."""
    async with NexusClient(LIVE_HOST) as client:
        ext = f"uuid:{uuid.uuid4()}"
        first = await client.create_node_with_external_id(
            labels=["LiveTestReplace"],
            properties={"colour": "red", "version": 1},
            external_id=ext,
        )
        assert first.error is None

        second = await client.create_node_with_external_id(
            labels=["LiveTestReplace"],
            properties={"colour": "blue", "version": 2},
            external_id=ext,
            conflict_policy="replace",
        )
        assert second.error is None
        # REPLACE must keep the same internal id.
        assert second.node_id == first.node_id

        # Third call: verify the properties were actually updated (the
        # fd001344 regression was that prop-ptr was not written, so the
        # old values survived a REPLACE).
        lookup: GetNodeByExternalIdResponse = await client.get_node_by_external_id(ext)
        assert lookup.node is not None
        assert (
            lookup.node.properties.get("colour") == "blue"
        ), "REPLACE did not overwrite 'colour' — fd001344 regression"
        assert (
            lookup.node.properties.get("version") == 2
        ), "REPLACE did not overwrite 'version' — fd001344 regression"


# ---------------------------------------------------------------------------
# 2.4 — Cypher CREATE with _id literal round-trip
# ---------------------------------------------------------------------------


@pytest.mark.live
@pytest.mark.asyncio
async def test_cypher_create_with_id_literal_round_trip() -> None:
    """should project the prefixed-string form of the external id via RETURN n._id."""
    async with NexusClient(LIVE_HOST) as client:
        p = _pfx()
        cyp_id = f"sha256:{p}{'9' * (64 - len(p))}"
        result = await client.execute_cypher(
            f"CREATE (n:LiveTestCypher {{_id: '{cyp_id}', tag: 'phase10'}}) RETURN n._id"
        )
        assert result.error is None, f"Cypher error: {result.error}"
        assert result.rows, "Expected at least one row"
        returned_id = result.rows[0][0]
        assert (
            returned_id == cyp_id
        ), f"RETURN n._id did not project the prefixed string: {returned_id!r}"


# ---------------------------------------------------------------------------
# 2.5 — Length-cap validation
# ---------------------------------------------------------------------------


@pytest.mark.live
@pytest.mark.asyncio
async def test_str_too_long_is_rejected() -> None:
    """should set response.error when str external id payload exceeds 256 bytes.

    The server returns HTTP 200 with a populated ``error`` field for validation
    failures (no 4xx status). The test asserts the error field is non-None.
    """
    async with NexusClient(LIVE_HOST) as client:
        too_long = "str:" + ("a" * 257)
        resp = await client.create_node_with_external_id(
            labels=["LiveTestLengthCap"],
            properties={},
            external_id=too_long,
        )
        assert resp.error is not None, "Expected error for str > 256 bytes, got None"
        assert "too long" in resp.error.lower() or "invalid" in resp.error.lower()


@pytest.mark.live
@pytest.mark.asyncio
async def test_bytes_too_long_is_rejected() -> None:
    """should set response.error when bytes external id payload exceeds 64 bytes.

    65 hex-encoded bytes (130 hex chars) exceeds the 64-byte cap.
    Server returns HTTP 200 with error field set.
    """
    async with NexusClient(LIVE_HOST) as client:
        too_long = "bytes:" + ("ff" * 65)
        resp = await client.create_node_with_external_id(
            labels=["LiveTestLengthCap"],
            properties={},
            external_id=too_long,
        )
        assert resp.error is not None, "Expected error for bytes > 64 bytes, got None"
        assert "too long" in resp.error.lower() or "invalid" in resp.error.lower()


@pytest.mark.live
@pytest.mark.asyncio
async def test_uuid_empty_payload_is_rejected() -> None:
    """should set response.error when the uuid external id has an empty payload.

    Server returns HTTP 200 with error field set for malformed external ids.
    """
    async with NexusClient(LIVE_HOST) as client:
        resp = await client.create_node_with_external_id(
            labels=["LiveTestLengthCap"],
            properties={},
            external_id="uuid:",
        )
        assert resp.error is not None, "Expected error for empty uuid payload, got None"
        assert "invalid" in resp.error.lower() or "bad" in resp.error.lower()


# ---------------------------------------------------------------------------
# Absent-id guard (complement to 2.2)
# ---------------------------------------------------------------------------


@pytest.mark.live
@pytest.mark.asyncio
async def test_get_node_by_absent_external_id_returns_none() -> None:
    """should return node=None without raising when the external id was never inserted."""
    async with NexusClient(LIVE_HOST) as client:
        absent = f"uuid:{uuid.uuid4()}"
        lookup: GetNodeByExternalIdResponse = await client.get_node_by_external_id(
            absent
        )
        # Server contract: miss -> node is None, no HTTP error.
        assert lookup.node is None, f"Expected None for absent id, got {lookup.node!r}"
