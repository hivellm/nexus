"""HTTP fallback transport.

Wraps ``httpx.AsyncClient`` behind the same :class:`Transport`
interface the RPC path uses. The translation from wire-level command
names to HTTP routes is a thin hard-coded table — every HTTP route
the legacy Python client relied on has a mapping here.
"""

from __future__ import annotations

import base64
from typing import List, Optional

import httpx

from nexus_sdk.transport.command_map import json_to_nexus
from nexus_sdk.transport.endpoint import Endpoint
from nexus_sdk.transport.types import (
    NexusValue,
    Transport,
    TransportCredentials,
    TransportRequest,
    TransportResponse,
)


class HttpTransport(Transport):
    """HTTP transport wrapping :class:`httpx.AsyncClient`."""

    def __init__(
        self,
        endpoint: Endpoint,
        credentials: TransportCredentials,
        timeout_s: float = 30.0,
    ) -> None:
        self._endpoint = endpoint
        self._credentials = credentials
        self._base_url = endpoint.as_http_url()
        self._client = httpx.AsyncClient(
            timeout=httpx.Timeout(timeout_s),
            headers={"User-Agent": "nexus-sdk/1.0.0"},
        )

    async def execute(self, req: TransportRequest) -> TransportResponse:
        value = await self._dispatch(req.command, req.args)
        return TransportResponse(value=value)

    def describe(self) -> str:
        tag = "HTTPS" if self._endpoint.scheme == "https" else "HTTP"
        return f"{self._endpoint} ({tag})"

    def is_rpc(self) -> bool:
        return False

    async def close(self) -> None:
        await self._client.aclose()

    # ── Internals ──────────────────────────────────────────────────────

    def _auth_headers(self) -> dict:
        if self._credentials.api_key:
            return {"X-API-Key": self._credentials.api_key}
        if self._credentials.username and self._credentials.password:
            token = base64.b64encode(
                f"{self._credentials.username}:{self._credentials.password}".encode("utf-8")
            ).decode("ascii")
            return {"Authorization": f"Basic {token}"}
        return {}

    async def _dispatch(self, cmd: str, args: List[NexusValue]) -> NexusValue:
        url_base = self._base_url
        headers = self._auth_headers()
        if cmd == "CYPHER":
            query = _as_str(args, 0, "CYPHER")
            params = _nexus_to_plain(args[1]) if len(args) > 1 else None
            body = {"query": query, "parameters": params}
            resp = await self._client.post(f"{url_base}/cypher", json=body, headers=headers)
            return _http_json(resp)
        if cmd in ("PING", "HEALTH"):
            resp = await self._client.get(f"{url_base}/health", headers=headers)
            return _http_json(resp)
        if cmd == "STATS":
            resp = await self._client.get(f"{url_base}/stats", headers=headers)
            return _http_json(resp)
        if cmd == "DB_LIST":
            resp = await self._client.get(f"{url_base}/databases", headers=headers)
            return _http_json(resp)
        if cmd == "DB_CREATE":
            name = _as_str(args, 0, "DB_CREATE")
            resp = await self._client.post(
                f"{url_base}/databases", json={"name": name}, headers=headers
            )
            return _http_json(resp)
        if cmd == "DB_DROP":
            name = _as_str(args, 0, "DB_DROP")
            resp = await self._client.delete(f"{url_base}/databases/{name}", headers=headers)
            return _http_json(resp)
        if cmd == "DB_USE":
            name = _as_str(args, 0, "DB_USE")
            resp = await self._client.put(
                f"{url_base}/session/database", json={"name": name}, headers=headers
            )
            return _http_json(resp)
        if cmd == "DB_CURRENT":
            resp = await self._client.get(f"{url_base}/session/database", headers=headers)
            return _http_json(resp)
        if cmd == "LABELS":
            resp = await self._client.get(f"{url_base}/schema/labels", headers=headers)
            return _http_json(resp)
        if cmd == "REL_TYPES":
            resp = await self._client.get(
                f"{url_base}/schema/relationship-types", headers=headers
            )
            return _http_json(resp)
        if cmd == "EXPORT":
            fmt = _as_str(args, 0, "EXPORT")
            resp = await self._client.get(f"{url_base}/export?format={fmt}", headers=headers)
            return json_to_nexus({"format": fmt, "data": resp.text})
        if cmd == "IMPORT":
            fmt = _as_str(args, 0, "IMPORT")
            payload = _as_str(args, 1, "IMPORT")
            resp = await self._client.post(
                f"{url_base}/import?format={fmt}",
                content=payload,
                headers={**headers, "Content-Type": "text/plain"},
            )
            return _http_json(resp)

        raise ValueError(
            f"HTTP fallback does not know how to route '{cmd}' — add an entry to "
            "nexus_sdk/transport/http_transport.py"
        )


def _as_str(args: List[NexusValue], idx: int, cmd: str) -> str:
    if idx >= len(args):
        raise ValueError(f"HTTP fallback: '{cmd}' needs argument {idx}")
    v = args[idx]
    if v.kind != "Str":
        raise ValueError(f"HTTP fallback: '{cmd}' argument {idx} must be a string")
    return str(v.value)


def _nexus_to_plain(v: NexusValue) -> object:
    kind = v.kind
    if kind == "Null":
        return None
    if kind in ("Bool", "Int", "Float", "Str"):
        return v.value
    if kind == "Bytes":
        return list(v.value)
    if kind == "Array":
        return [_nexus_to_plain(x) for x in v.value]
    if kind == "Map":
        out: dict = {}
        for k, val in v.value:
            key = k.value if k.kind == "Str" else str(k.value)
            out[str(key)] = _nexus_to_plain(val)
        return out
    raise ValueError(f"unknown NexusValue kind '{kind}'")


def _http_json(resp: httpx.Response) -> NexusValue:
    if resp.status_code >= 400:
        try:
            text = resp.text
        except Exception:
            text = f"HTTP {resp.status_code}"
        raise RuntimeError(f"HTTP {resp.status_code}: {text}")
    try:
        data = resp.json()
    except Exception:
        data = resp.text
    return json_to_nexus(data)


# The annotation types used by the dispatch return signature live here
# so callers can hint at :class:`NexusValue` without importing the
# internal helper.
_UNUSED_OPTIONAL = Optional
