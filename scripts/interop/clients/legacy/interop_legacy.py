#!/usr/bin/env python3
"""Interop cell: a pre-Thunder client against a Thunder-based server.

This is the compatibility cell. It deliberately imports no Nexus SDK -- it
hand-writes the wire exactly as the SDKs emitted it before the Thunder swap, so
it keeps testing the old encoding even after every SDK has moved on and no
pre-Thunder build is left to check out:

  * requests are **map-shaped** (``{"id", "command", "args"}``) rather than the
    array form Thunder emits -- the server's rmp-serde decode tolerates both
    (WIRE-013);
  * ``Bytes`` are an **array of integers** rather than MessagePack ``bin``
    (WIRE-011), the shape the pre-Thunder Rust/serde clients produced.

A green cell here means an old client in the wild keeps working against the
migrated server. A red one means the release breaks deployed software.

No casualties: the four steps below all pass on the current server because the
tolerant decode path accepts the legacy framing intact. If a future server
tightened the decoder, this cell is where that regression would surface.
"""

from __future__ import annotations

import socket
import struct
import sys

import msgpack

# f32 little-endian, non-UTF-8 on purpose -- see clients/python/interop.py.
VEC = [1.5, -2.5, 3.5, float("inf")]
VEC_BYTES = b"".join(struct.pack("<f", x) for x in VEC)


def report(step: str, ok: bool, detail: str) -> None:
    print(f"STEP {step} {'PASS' if ok else 'FAIL'} {detail}", flush=True)


class LegacyConn:
    """The pre-Thunder framing: 4-byte LE length prefix + MessagePack body."""

    def __init__(self, host: str, port: int) -> None:
        self.sock = socket.create_connection((host, port), timeout=15)
        self.next_id = 0

    def send(self, cmd: str, args: list) -> int:
        self.next_id += 1
        frame_id = self.next_id
        # Map-shaped request: what the old SDKs emitted.
        body = msgpack.packb(
            {"id": frame_id, "command": cmd.upper(), "args": args},
            use_bin_type=True,
        )
        self.sock.sendall(struct.pack("<I", len(body)) + body)
        return frame_id

    def recv(self) -> tuple[int, dict]:
        (length,) = struct.unpack("<I", self._read_exactly(4))
        decoded = msgpack.unpackb(
            self._read_exactly(length), raw=False, strict_map_key=False
        )
        # Responses are array-shaped [id, {"Ok"|"Err": ...}]; older map-shaped
        # {"id", "result"} is handled too for symmetry with the request path.
        if isinstance(decoded, list):
            return decoded[0], decoded[1]
        return decoded["id"], decoded["result"]

    def call(self, cmd: str, args: list) -> dict:
        self.send(cmd, args)
        _frame_id, result = self.recv()
        return result

    def _read_exactly(self, n: int) -> bytes:
        buf = b""
        while len(buf) < n:
            chunk = self.sock.recv(n - len(buf))
            if not chunk:
                raise ConnectionError("server closed the connection")
            buf += chunk
        return buf

    def close(self) -> None:
        self.sock.close()


# --- externally-tagged Value encoders (the wire the old clients spoke) --------
def wire_str(s: str) -> dict:
    return {"Str": s}


def wire_int(n: int) -> dict:
    return {"Int": n}


def wire_bytes_legacy(b: bytes) -> dict:
    """`Bytes` as an array of integers -- the pre-Thunder encoding."""
    return {"Bytes": list(b)}


def wire_map(pairs: list[tuple[dict, dict]]) -> dict:
    return {"Map": [[k, v] for k, v in pairs]}


def untag(value):
    """Collapse an externally-tagged Value into a native Python value."""
    if value == "Null":
        return None
    if not isinstance(value, dict) or len(value) != 1:
        return value
    (kind, inner), = value.items()
    if kind in ("Str", "Int", "Float", "Bool"):
        return inner
    if kind == "Bytes":
        return bytes(inner) if isinstance(inner, list) else inner
    if kind == "Array":
        return [untag(x) for x in inner]
    if kind == "Map":
        return {untag(k): untag(v) for k, v in inner}
    return value


def cypher_rows(conn: LegacyConn, query: str, params: list[tuple[dict, dict]]) -> list:
    args = [wire_str(query)]
    if params:
        args.append(wire_map(params))
    result = conn.call("CYPHER", args)
    if "Err" in result:
        raise RuntimeError(result["Err"])
    envelope = untag(result.get("Ok"))
    rows = envelope.get("rows") if isinstance(envelope, dict) else None
    return rows if isinstance(rows, list) else []


def main() -> int:
    host, port, user, password = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
    failures = 0
    conn = LegacyConn(host, port)

    # 1. auth -- PING answers before auth; STATS is refused (NOAUTH) until AUTH,
    #    then succeeds. All frames map-shaped, as the old clients sent them.
    ping = conn.call("PING", [])
    ping_ok = untag(ping.get("Ok")) == "PONG"
    stats_pre = conn.call("STATS", [])
    stats_pre_refused = "Err" in stats_pre and "auth" in stats_pre["Err"].lower()
    auth = conn.call("AUTH", [wire_str(user), wire_str(password)])
    auth_ok = "Ok" in auth
    stats_post = conn.call("STATS", [])
    stats_post_ok = "Ok" in stats_post
    ok = ping_ok and stats_pre_refused and auth_ok and stats_post_ok
    report(
        "auth",
        ok,
        f"ping={ping_ok} stats_pre_refused={stats_pre_refused} "
        f"auth={auth_ok} stats_post={stats_post_ok}",
    )
    failures += 0 if ok else 1
    if not auth_ok:
        conn.close()
        return 1

    # 2. cypher -- CREATE then MATCH round-trips the id back.
    marker = 424206
    try:
        cypher_rows(
            conn,
            "CREATE (n:InteropLegacy {id: $id}) RETURN n.id",
            [(wire_str("id"), wire_int(marker))],
        )
        rows = cypher_rows(
            conn,
            "MATCH (n:InteropLegacy {id: $id}) RETURN n.id",
            [(wire_str("id"), wire_int(marker))],
        )
        got = rows[0][0] if rows and rows[0] else None
        ok = int(got) == marker if got is not None else False
        report("cypher", ok, f"round-trip id -> {got!r}")
        failures += 0 if ok else 1
    except Exception as exc:  # noqa: BLE001
        report("cypher", False, f"{type(exc).__name__}: {exc}")
        failures += 1

    # 3. knn_bytes -- Bytes sent in the legacy int-array form come back in the
    #    server's canonical `bin` form (msgpack hands us Python bytes), byte-exact.
    try:
        echoed = conn.call("PING", [wire_bytes_legacy(VEC_BYTES)])
        got = untag(echoed.get("Ok"))
        got = bytes(got) if isinstance(got, (bytes, list)) else b""
        ok = got == VEC_BYTES and len(VEC_BYTES) == 4 * len(VEC)
        report("knn_bytes", ok, f"{VEC_BYTES.hex()} -> {got.hex()}")
        failures += 0 if ok else 1
    except Exception as exc:  # noqa: BLE001
        report("knn_bytes", False, f"{type(exc).__name__}: {exc}")
        failures += 1

    # 4. error -- a broken CYPHER comes back as Err, and the connection stays up.
    try:
        broken = conn.call("CYPHER", [wire_str("MATCH (n RETURN")])
        errored = "Err" in broken
        alive = untag(conn.call("PING", []).get("Ok")) == "PONG"
        ok = errored and alive
        report("error", ok, f"{broken}; connection alive={alive}")
        failures += 0 if ok else 1
    except Exception as exc:  # noqa: BLE001
        report("error", False, f"{type(exc).__name__}: {exc}")
        failures += 1

    conn.close()
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
