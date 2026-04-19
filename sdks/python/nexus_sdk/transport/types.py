"""Shared transport-layer primitives.

Mirrors ``sdks/typescript/src/transports/types.ts`` and
``sdks/rust/src/transport/mod.rs`` — the same tagged ``NexusValue``
union, the same ``TransportMode`` tokens, and the same ``Transport``
interface.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, List, Optional, Tuple, Union


class TransportMode(str, Enum):
    """Transport selector.

    Values match the URL-scheme tokens and the ``NEXUS_SDK_TRANSPORT``
    env-var strings so a single token lines up everywhere.
    """

    NEXUS = "nexus"
    RESP3 = "resp3"
    HTTP = "http"
    HTTPS = "https"

    @classmethod
    def parse(cls, raw: str) -> Optional["TransportMode"]:
        """Parse the ``NEXUS_SDK_TRANSPORT`` env-var token.

        Accepts the canonical values plus the ``rpc`` / ``nexusrpc``
        aliases for ergonomics. Returns ``None`` for empty / ``auto``.
        """
        v = raw.strip().lower()
        if v in ("nexus", "rpc", "nexusrpc"):
            return cls.NEXUS
        if v == "resp3":
            return cls.RESP3
        if v == "http":
            return cls.HTTP
        if v == "https":
            return cls.HTTPS
        return None

    @property
    def is_rpc(self) -> bool:
        return self is TransportMode.NEXUS


# ── NexusValue ──────────────────────────────────────────────────────────
#
# Python does not have native tagged unions, so we represent NexusValue
# as a dataclass with a string ``kind`` discriminator plus a ``value``
# field. The codec (see ``codec.py``) translates between this shape and
# the externally-tagged MessagePack format rmp-serde uses on the wire.


@dataclass(frozen=True)
class NexusValue:
    """Dynamically-typed value carried by RPC requests and responses."""

    kind: str
    value: Any = None

    def __repr__(self) -> str:  # pragma: no cover - debug aid
        return f"nx.{self.kind}({self.value!r})" if self.kind != "Null" else "nx.Null()"


class _NxFactory:
    """Helper constructors — shorter at call sites than ``NexusValue(kind=...)``."""

    @staticmethod
    def Null() -> NexusValue:
        return NexusValue("Null")

    @staticmethod
    def Bool(v: bool) -> NexusValue:
        return NexusValue("Bool", bool(v))

    @staticmethod
    def Int(v: int) -> NexusValue:
        return NexusValue("Int", int(v))

    @staticmethod
    def Float(v: float) -> NexusValue:
        return NexusValue("Float", float(v))

    @staticmethod
    def Bytes(v: bytes) -> NexusValue:
        return NexusValue("Bytes", bytes(v))

    @staticmethod
    def Str(v: str) -> NexusValue:
        return NexusValue("Str", str(v))

    @staticmethod
    def Array(v: List[NexusValue]) -> NexusValue:
        return NexusValue("Array", list(v))

    @staticmethod
    def Map(v: List[Tuple[NexusValue, NexusValue]]) -> NexusValue:
        return NexusValue("Map", list(v))


nx = _NxFactory()


# ── Transport interface ────────────────────────────────────────────────


@dataclass
class TransportCredentials:
    """Credentials carried by a transport.

    Both paths may be set; ``api_key`` takes precedence.
    """

    api_key: Optional[str] = None
    username: Optional[str] = None
    password: Optional[str] = None

    def has_any(self) -> bool:
        return bool(self.api_key) or (bool(self.username) and bool(self.password))


@dataclass
class TransportRequest:
    """A single request against the active transport."""

    command: str
    args: List[NexusValue] = field(default_factory=list)


@dataclass
class TransportResponse:
    """A single response from the active transport."""

    value: NexusValue


class Transport(ABC):
    """Generic transport interface — one method per request/response pair."""

    @abstractmethod
    async def execute(self, req: TransportRequest) -> TransportResponse:
        """Send a single request and wait for the matching response."""

    @abstractmethod
    def describe(self) -> str:
        """Short human-readable description (e.g. ``nexus://host:15475 (RPC)``)."""

    @abstractmethod
    def is_rpc(self) -> bool:
        """True when the active transport uses the native binary RPC wire format."""

    @abstractmethod
    async def close(self) -> None:
        """Release any persistent sockets."""


# Convenience type alias used across the transport layer.
NexusValueLike = Union[NexusValue, None, bool, int, float, str, bytes, list, dict]
