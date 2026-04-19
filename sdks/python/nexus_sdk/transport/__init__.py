"""Transport layer for the Nexus Python SDK.

Every `NexusClient` in 1.0.0 delegates its wire format to a `Transport`
picked at construction time. Three modes are recognised:

- ``nexus`` — native binary RPC (length-prefixed MessagePack on port
  15475). **Default**.
- ``http`` / ``https`` — JSON over REST on port 15474 / 443. Legacy /
  browser-friendly.
- ``resp3`` — reserved for a future RESP3 implementation. Currently
  raises :class:`ConfigurationError`.

Precedence when picking the transport:

1. URL scheme in ``base_url`` (``nexus://`` -> RPC, ``http://`` -> HTTP, ...)
2. ``NEXUS_SDK_TRANSPORT`` env var
3. ``transport`` field passed to :class:`NexusClient`
4. Default: ``nexus``

See ``docs/specs/sdk-transport.md`` for the cross-SDK contract.
"""

from nexus_sdk.transport.command_map import map_command
from nexus_sdk.transport.endpoint import (
    Endpoint,
    HTTP_DEFAULT_PORT,
    HTTPS_DEFAULT_PORT,
    RESP3_DEFAULT_PORT,
    RPC_DEFAULT_PORT,
    default_local_endpoint,
    parse_endpoint,
)
from nexus_sdk.transport.factory import build_transport
from nexus_sdk.transport.http_transport import HttpTransport
from nexus_sdk.transport.rpc import RpcTransport
from nexus_sdk.transport.types import (
    NexusValue,
    Transport,
    TransportCredentials,
    TransportMode,
    TransportRequest,
    TransportResponse,
    nx,
)

__all__ = [
    "Endpoint",
    "HttpTransport",
    "NexusValue",
    "RpcTransport",
    "Transport",
    "TransportCredentials",
    "TransportMode",
    "TransportRequest",
    "TransportResponse",
    "build_transport",
    "default_local_endpoint",
    "map_command",
    "nx",
    "parse_endpoint",
    "HTTP_DEFAULT_PORT",
    "HTTPS_DEFAULT_PORT",
    "RESP3_DEFAULT_PORT",
    "RPC_DEFAULT_PORT",
]
