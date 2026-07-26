#!/usr/bin/env python3
"""Interop cell: Python SDK (hivellm-thunder) against a Thunder-based server.

Drives ``RpcTransport`` directly rather than the sugar layer — the matrix is
about the wire, and the transport is where the wire lives.

    argv:   <host> <port> <user> <pass>
    stdout: one `STEP <name> PASS|FAIL <detail>` line per step
    exit:   0 iff every step passed
"""

from __future__ import annotations

import asyncio
import struct
import sys

from nexus_sdk.transport.endpoint import Endpoint
from nexus_sdk.transport.rpc import RpcTransport
from nexus_sdk.transport.types import NexusValue, TransportCredentials, TransportRequest, nx

# A vector whose f32-LE encoding is emphatically not valid UTF-8, so a transport
# that quietly round-trips Bytes through a string cannot pass the knn_bytes cell.
VEC = [1.5, -2.5, 3.5, float("inf")]
VEC_BYTES = b"".join(struct.pack("<f", x) for x in VEC)


def report(step: str, ok: bool, detail: str) -> None:
    print(f"STEP {step} {'PASS' if ok else 'FAIL'} {detail}", flush=True)


def map_get(v: NexusValue, key: str) -> NexusValue | None:
    """Look up a string key in a Map-kind NexusValue."""
    if v.kind != "Map":
        return None
    for pair in v.value:
        k, val = pair
        if k.kind == "Str" and k.value == key:
            return val
    return None


async def cypher_rows(t: RpcTransport, query: str, params: dict) -> list[list]:
    """Run CYPHER over the transport and return rows as native Python lists."""
    args = [nx.Str(query)]
    if params:
        args.append(nx.Map([(nx.Str(k), _to_nx(v)) for k, v in params.items()]))
    resp = await t.execute(TransportRequest(command="CYPHER", args=args))
    err = map_get(resp.value, "error")
    if err is not None and err.kind == "Str" and err.value:
        raise RuntimeError(err.value)
    rows = map_get(resp.value, "rows")
    if rows is None or rows.kind != "Array":
        return []
    return [[_from_nx(cell) for cell in row.value] for row in rows.value if row.kind == "Array"]


def _to_nx(v) -> NexusValue:
    if isinstance(v, bool):
        return nx.Bool(v)
    if isinstance(v, int):
        return nx.Int(v)
    if isinstance(v, float):
        return nx.Float(v)
    if isinstance(v, bytes):
        return nx.Bytes(v)
    return nx.Str(str(v))


def _from_nx(v: NexusValue):
    return v.value


async def main() -> int:
    host, port, user, password = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
    endpoint = Endpoint(scheme="nexus", host=host, port=port)
    failures = 0

    # 1. auth — PING answers before auth; STATS is refused before AUTH and
    #    succeeds after. An unauthenticated transport is one built with no creds.
    try:
        anon = RpcTransport(endpoint, TransportCredentials())
        pong = await anon.execute(TransportRequest(command="PING", args=[]))
        ping_ok = pong.value.kind == "Str" and pong.value.value == "PONG"
        stats_pre_refused = False
        try:
            await anon.execute(TransportRequest(command="STATS", args=[]))
        except Exception as exc:  # noqa: BLE001 — any typed refusal is fine
            stats_pre_refused = "auth" in str(exc).lower() or "NOAUTH" in str(exc)
        await anon.close()

        authed = RpcTransport(
            endpoint, TransportCredentials(username=user, password=password)
        )
        stats = await authed.execute(TransportRequest(command="STATS", args=[]))
        stats_post_ok = stats.value.kind in ("Map", "Str")

        ok = ping_ok and stats_pre_refused and stats_post_ok
        report("auth", ok, f"ping={ping_ok} stats_pre_refused={stats_pre_refused} stats_post={stats_post_ok}")
        failures += 0 if ok else 1
    except Exception as exc:  # noqa: BLE001
        report("auth", False, f"{type(exc).__name__}: {exc}")
        return 1

    # 2. cypher — CREATE then MATCH round-trips the id back.
    marker = 424242
    try:
        await cypher_rows(authed, "CREATE (n:InteropPy {id: $id}) RETURN n.id", {"id": marker})
        rows = await cypher_rows(authed, "MATCH (n:InteropPy {id: $id}) RETURN n.id", {"id": marker})
        got = rows[0][0] if rows and rows[0] else None
        ok = int(got) == marker if got is not None else False
        report("cypher", ok, f"round-trip id -> {got!r}")
        failures += 0 if ok else 1
    except Exception as exc:  # noqa: BLE001
        report("cypher", False, f"{type(exc).__name__}: {exc}")
        failures += 1

    # 3. knn_bytes — a raw f32-LE vector carried as Bytes round-trips byte-exact
    #    (PING echoes its argument), and the client's own float encoding agrees.
    try:
        echoed = await authed.execute(TransportRequest(command="PING", args=[nx.Bytes(VEC_BYTES)]))
        got = echoed.value.value if echoed.value.kind == "Bytes" else b""
        ok = got == VEC_BYTES and len(VEC_BYTES) == 4 * len(VEC)
        report("knn_bytes", ok, f"{VEC_BYTES.hex()} -> {bytes(got).hex()}")
        failures += 0 if ok else 1
    except Exception as exc:  # noqa: BLE001
        report("knn_bytes", False, f"{type(exc).__name__}: {exc}")
        failures += 1

    # 4. error — a broken CYPHER surfaces a typed server error, not a transport
    #    crash, and the same connection stays usable.
    try:
        try:
            await cypher_rows(authed, "MATCH (n RETURN", {})
            report("error", False, "expected a server error, got a result")
            failures += 1
        except Exception as exc:  # noqa: BLE001 — expected
            pong = await authed.execute(TransportRequest(command="PING", args=[]))
            alive = pong.value.kind == "Str" and pong.value.value == "PONG"
            report("error", alive, f"raised {type(exc).__name__}; connection alive={alive}")
            failures += 0 if alive else 1
    finally:
        await authed.close()

    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))
