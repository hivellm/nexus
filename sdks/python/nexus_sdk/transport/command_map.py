"""SDK dotted-name -> wire-command mapping.

Every method :class:`NexusClient` exposes (``execute_cypher``,
``list_databases``, ``ping``, ...) funnels through
:func:`map_command`. The table must stay in sync with
``docs/specs/sdk-transport.md §6`` and with the Rust SDK's
``sdks/rust/src/transport/command_map.rs``.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, List, Mapping, Optional

from nexus_sdk.transport.types import NexusValue, nx


@dataclass
class CommandMapping:
    command: str
    args: List[NexusValue]


def map_command(dotted: str, payload: Mapping[str, Any]) -> Optional[CommandMapping]:
    """Translate an SDK dotted name into a wire command + argument vector.

    Returns ``None`` for unknown names so the client can fall back to
    the HTTP transport (if configured) or surface a clear error.
    """
    if dotted == "graph.cypher":
        query = payload.get("query")
        if not isinstance(query, str):
            return None
        args: List[NexusValue] = [nx.Str(query)]
        params = payload.get("parameters")
        if params is not None:
            args.append(json_to_nexus(params))
        return CommandMapping("CYPHER", args)
    if dotted == "graph.ping":
        return CommandMapping("PING", [])
    if dotted == "graph.hello":
        return CommandMapping("HELLO", [nx.Int(1)])
    if dotted == "graph.stats":
        return CommandMapping("STATS", [])
    if dotted == "graph.health":
        return CommandMapping("HEALTH", [])
    if dotted == "graph.quit":
        return CommandMapping("QUIT", [])
    if dotted == "auth.login":
        api_key = payload.get("api_key")
        if isinstance(api_key, str) and api_key:
            return CommandMapping("AUTH", [nx.Str(api_key)])
        user = payload.get("username")
        pw = payload.get("password")
        if not isinstance(user, str) or not isinstance(pw, str):
            return None
        return CommandMapping("AUTH", [nx.Str(user), nx.Str(pw)])

    if dotted in ("db.list",):
        return CommandMapping("DB_LIST", [])
    if dotted in ("db.create", "db.drop", "db.use"):
        name = payload.get("name")
        if not isinstance(name, str):
            return None
        cmd = {"db.create": "DB_CREATE", "db.drop": "DB_DROP", "db.use": "DB_USE"}[
            dotted
        ]
        return CommandMapping(cmd, [nx.Str(name)])

    if dotted == "schema.labels":
        return CommandMapping("LABELS", [])
    if dotted == "schema.rel_types":
        return CommandMapping("REL_TYPES", [])
    if dotted == "schema.property_keys":
        return CommandMapping("PROPERTY_KEYS", [])
    if dotted == "schema.indexes":
        return CommandMapping("INDEXES", [])

    if dotted == "data.export":
        fmt = payload.get("format")
        if not isinstance(fmt, str):
            return None
        args = [nx.Str(fmt)]
        query = payload.get("query")
        if isinstance(query, str):
            args.append(nx.Str(query))
        return CommandMapping("EXPORT", args)
    if dotted == "data.import":
        fmt = payload.get("format")
        data = payload.get("data")
        if not isinstance(fmt, str) or not isinstance(data, str):
            return None
        return CommandMapping("IMPORT", [nx.Str(fmt), nx.Str(data)])

    return None


def json_to_nexus(v: Any) -> NexusValue:
    """JSON-compatible Python value -> :class:`NexusValue`."""
    if v is None:
        return nx.Null()
    if isinstance(v, bool):
        return nx.Bool(v)
    if isinstance(v, int):
        return nx.Int(v)
    if isinstance(v, float):
        return nx.Float(v)
    if isinstance(v, str):
        return nx.Str(v)
    if isinstance(v, (bytes, bytearray, memoryview)):
        return nx.Bytes(bytes(v))
    if isinstance(v, (list, tuple)):
        return nx.Array([json_to_nexus(x) for x in v])
    if isinstance(v, dict):
        return nx.Map([(nx.Str(str(k)), json_to_nexus(val)) for k, val in v.items()])
    raise TypeError(f"cannot encode {type(v).__name__} as NexusValue")


def nexus_to_json(v: NexusValue) -> Any:
    """:class:`NexusValue` -> plain Python value for user-visible surfaces."""
    kind = v.kind
    if kind == "Null":
        return None
    if kind in ("Bool", "Int", "Float", "Str", "Bytes"):
        return v.value
    if kind == "Array":
        return [nexus_to_json(x) for x in v.value]
    if kind == "Map":
        out: dict = {}
        for k, val in v.value:
            if k.kind == "Str":
                key = k.value
            elif k.kind == "Int":
                key = str(k.value)
            else:
                key = repr(nexus_to_json(k))
            out[key] = nexus_to_json(val)
        return out
    raise ValueError(f"nexus_to_json: unknown kind '{kind}'")
