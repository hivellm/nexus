"""Transport factory — applies the precedence chain.

URL scheme  >  NEXUS_SDK_TRANSPORT env var  >  config field  >  default (nexus)
"""

from __future__ import annotations

import os
from dataclasses import dataclass
from typing import Optional

from nexus_sdk.transport.endpoint import (
    Endpoint,
    HTTP_DEFAULT_PORT,
    HTTPS_DEFAULT_PORT,
    RESP3_DEFAULT_PORT,
    RPC_DEFAULT_PORT,
    default_local_endpoint,
    parse_endpoint,
)
from nexus_sdk.transport.http_transport import HttpTransport
from nexus_sdk.transport.rpc import RpcTransport
from nexus_sdk.transport.types import (
    Transport,
    TransportCredentials,
    TransportMode,
)


@dataclass
class BuiltTransport:
    transport: Transport
    endpoint: Endpoint
    mode: TransportMode


def build_transport(
    base_url: Optional[str],
    credentials: TransportCredentials,
    transport_hint: Optional[TransportMode] = None,
    *,
    rpc_port: Optional[int] = None,
    resp3_port: Optional[int] = None,
    timeout_s: float = 30.0,
    env_transport: Optional[str] = None,
) -> BuiltTransport:
    """Resolve the effective transport given the precedence chain.

    ``env_transport`` is injected so the tests can exercise the env-var
    path without mutating ``os.environ``. Defaults to
    ``NEXUS_SDK_TRANSPORT``.
    """
    endpoint = parse_endpoint(base_url) if base_url else default_local_endpoint()

    # 1. URL scheme wins.
    mode = _scheme_to_mode(endpoint.scheme)

    # 2. Env var overrides a bare URL (no scheme).
    explicit_scheme = base_url is not None and "://" in base_url
    env_raw = env_transport if env_transport is not None else os.environ.get("NEXUS_SDK_TRANSPORT")
    env_mode = TransportMode.parse(env_raw) if env_raw else None
    if env_mode and not explicit_scheme:
        mode = env_mode
        endpoint = _realign_endpoint(endpoint, mode, rpc_port, resp3_port)

    # 3. Config field.
    if transport_hint and not explicit_scheme and not env_mode:
        mode = transport_hint
        endpoint = _realign_endpoint(endpoint, mode, rpc_port, resp3_port)

    if mode is TransportMode.NEXUS:
        return BuiltTransport(
            transport=RpcTransport(endpoint, credentials),
            endpoint=endpoint,
            mode=mode,
        )
    if mode in (TransportMode.HTTP, TransportMode.HTTPS):
        return BuiltTransport(
            transport=HttpTransport(endpoint, credentials, timeout_s=timeout_s),
            endpoint=endpoint,
            mode=mode,
        )
    if mode is TransportMode.RESP3:
        raise ValueError(
            "resp3 transport is not yet shipped in the Python SDK — use 'nexus' (RPC) "
            "or 'http' for now"
        )
    raise ValueError(f"unknown transport mode: {mode}")


def _scheme_to_mode(scheme: str) -> TransportMode:
    if scheme == "nexus":
        return TransportMode.NEXUS
    if scheme == "http":
        return TransportMode.HTTP
    if scheme == "https":
        return TransportMode.HTTPS
    if scheme == "resp3":
        return TransportMode.RESP3
    raise ValueError(f"unknown URL scheme: {scheme}")


def _realign_endpoint(
    ep: Endpoint,
    mode: TransportMode,
    rpc_port: Optional[int],
    resp3_port: Optional[int],
) -> Endpoint:
    """Retarget the port when the mode changes out from under the URL."""
    if mode is TransportMode.NEXUS:
        return Endpoint(scheme="nexus", host=ep.host, port=rpc_port or RPC_DEFAULT_PORT)
    if mode is TransportMode.RESP3:
        return Endpoint(scheme="resp3", host=ep.host, port=resp3_port or RESP3_DEFAULT_PORT)
    if mode is TransportMode.HTTPS:
        return Endpoint(scheme="https", host=ep.host, port=HTTPS_DEFAULT_PORT)
    return Endpoint(scheme="http", host=ep.host, port=HTTP_DEFAULT_PORT)
