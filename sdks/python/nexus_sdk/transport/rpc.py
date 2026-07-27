"""Native binary RPC transport — a thin wrapper around the Thunder client.

Thunder (``hivellm-thunder``) owns the wire (frame + MessagePack codec),
the single-connection multiplexer, and the handshake / reconnect /
credential re-send. This module adapts the SDK's value model
(``NexusValue``, capitalized kinds) to Thunder's (``Value``, lowercase
kinds) and preserves the ``Transport`` interface the rest of the SDK uses.

Handshake: the Nexus server ignores ``HELLO`` arguments and gates the
connection on a separate ``AUTH``, so Thunder's ``AUTH_COMMAND`` handshake
with the ``ARG_LESS`` HELLO style (bare ``HELLO []`` then ``AUTH [...]``)
matches the wire exactly and re-authenticates on reconnect.
"""

from __future__ import annotations

import asyncio
from typing import Optional

from thunder_rpc import AsyncClient, Config, Value
from thunder_rpc.client_config import ClientConfig, Credentials
from thunder_rpc.config import Handshake, HelloStyle

from nexus_sdk.transport.endpoint import Endpoint
from nexus_sdk.transport.types import (
    NexusValue,
    Transport,
    TransportCredentials,
    TransportRequest,
    TransportResponse,
    nx,
)


def _nexus_to_thunder(v: NexusValue) -> Value:
    """Translate the SDK's ``NexusValue`` into a Thunder ``Value``."""
    kind = v.kind
    if kind == "Null":
        return Value.null()
    if kind == "Bool":
        return Value.bool(v.value)
    if kind == "Int":
        return Value.int(v.value)
    if kind == "Float":
        return Value.float(v.value)
    if kind == "Bytes":
        return Value.bytes(v.value)
    if kind == "Str":
        return Value.str(v.value)
    if kind == "Array":
        return Value.array([_nexus_to_thunder(x) for x in v.value])
    if kind == "Map":
        return Value.map(
            [(_nexus_to_thunder(a), _nexus_to_thunder(b)) for a, b in v.value]
        )
    raise ValueError(f"unknown NexusValue kind: {kind!r}")


def _thunder_to_nexus(v: Value) -> NexusValue:
    """Translate a Thunder ``Value`` back into the SDK's ``NexusValue``."""
    kind = v.kind
    if kind == "null":
        return nx.Null()
    if kind == "bool":
        return nx.Bool(v.value)
    if kind == "int":
        return nx.Int(v.value)
    if kind == "float":
        return nx.Float(v.value)
    if kind == "bytes":
        return nx.Bytes(v.value)
    if kind == "str":
        return nx.Str(v.value)
    if kind == "array":
        return nx.Array([_thunder_to_nexus(x) for x in v.value])
    if kind == "map":
        return nx.Map(
            [(_thunder_to_nexus(a), _thunder_to_nexus(b)) for a, b in v.value]
        )
    raise ValueError(f"unknown Thunder Value kind: {kind!r}")


class RpcTransport(Transport):
    """Asyncio RPC transport backed by a single ``AsyncClient``."""

    def __init__(
        self,
        endpoint: Endpoint,
        credentials: TransportCredentials,
        connect_timeout_s: float = 5.0,
    ) -> None:
        self._endpoint = endpoint
        self._credentials = credentials
        self._connect_timeout_s = connect_timeout_s
        self._client: Optional[AsyncClient] = None
        self._connect_lock = asyncio.Lock()

    async def execute(self, req: TransportRequest) -> TransportResponse:
        client = await self._ensure_connected()
        # ``call`` returns the ``Ok`` payload directly and raises a typed
        # ``ThunderError`` on ``Err``; the SDK's higher layers surface those.
        result = await client.call(
            req.command, [_nexus_to_thunder(a) for a in req.args]
        )
        return TransportResponse(value=_thunder_to_nexus(result))

    def describe(self) -> str:
        return f"{self._endpoint} (RPC)"

    def is_rpc(self) -> bool:
        return True

    async def close(self) -> None:
        client = self._client
        self._client = None
        if client is not None:
            await client.close()

    # ── Internals ──────────────────────────────────────────────────────

    def _config(self) -> Config:
        return (
            Config.standard()
            .with_scheme("nexus")
            .with_port(self._endpoint.port)
            .with_handshake(Handshake.AUTH_COMMAND)
            .with_hello_style(HelloStyle.ARG_LESS)
        )

    def _thunder_credentials(self) -> Optional[Credentials]:
        if self._credentials.api_key:
            return Credentials.api_key(self._credentials.api_key)
        if self._credentials.username and self._credentials.password:
            return Credentials.user_pass(
                self._credentials.username, self._credentials.password
            )
        return None

    async def _ensure_connected(self) -> AsyncClient:
        if self._client is not None:
            return self._client
        async with self._connect_lock:
            if self._client is not None:
                return self._client
            # Bare ``host:port`` endpoint sidesteps Thunder's scheme matching.
            self._client = await AsyncClient.connect(
                f"{self._endpoint.host}:{self._endpoint.port}",
                self._config(),
                ClientConfig(
                    connect_timeout=self._connect_timeout_s,
                    credentials=self._thunder_credentials(),
                ),
            )
            return self._client
