"""Wire codec for the Nexus RPC protocol.

The server-side types live in ``nexus_protocol::rpc::types``. rmp-serde
encodes Rust enums using the externally-tagged representation by
default, which means:

- Unit variants (``NexusValue::Null``) -> the string ``"Null"``.
- Data-bearing variants (``NexusValue::Str("hi")``) -> a single-key map
  ``{"Str": "hi"}``.
- ``Result<T, E>`` follows the same rule: ``{"Ok": v}`` / ``{"Err": s}``.

``to_wire_value`` / ``from_wire_value`` translate between the tagged
Python type and that on-wire shape. ``encode_request_frame`` and
``decode_response_body`` handle the ``u32 LE length prefix + MessagePack``
frame format documented in ``docs/specs/rpc-wire-format.md``.
"""

from __future__ import annotations

import struct
from dataclasses import dataclass
from typing import Any, Tuple, Union

import msgpack  # type: ignore[import-untyped]

from nexus_sdk.transport.types import NexusValue, nx

# rmp-serde encodes Rust's `Result<Ok, Err>` as `{"Ok": v}` / `{"Err": s}`.
_OK_TAG = "Ok"
_ERR_TAG = "Err"


def to_wire_value(v: NexusValue) -> Any:
    """Encode a :class:`NexusValue` into its on-wire (pre-MessagePack) JS shape."""
    kind = v.kind
    if kind == "Null":
        return "Null"
    if kind == "Bool":
        return {"Bool": bool(v.value)}
    if kind == "Int":
        return {"Int": int(v.value)}
    if kind == "Float":
        return {"Float": float(v.value)}
    if kind == "Bytes":
        return {"Bytes": bytes(v.value)}
    if kind == "Str":
        return {"Str": str(v.value)}
    if kind == "Array":
        return {"Array": [to_wire_value(x) for x in v.value]}
    if kind == "Map":
        # rmp-serde encodes Vec<(K, V)> as an array of 2-tuples — NOT a
        # msgpack map, since keys may be non-string NexusValues.
        return {"Map": [[to_wire_value(k), to_wire_value(val)] for (k, val) in v.value]}
    raise ValueError(f"unknown NexusValue kind '{kind}'")


def from_wire_value(raw: Any) -> NexusValue:
    """Decode the wire-level Python shape back into a tagged :class:`NexusValue`."""
    if raw == "Null" or raw is None:
        return nx.Null()
    if isinstance(raw, bool):
        return nx.Bool(raw)
    if isinstance(raw, int):
        return nx.Int(raw)
    if isinstance(raw, float):
        return nx.Float(raw)
    if isinstance(raw, (bytes, bytearray, memoryview)):
        return nx.Bytes(bytes(raw))
    if isinstance(raw, str):
        return nx.Str(raw)
    if isinstance(raw, list):
        return nx.Array([from_wire_value(x) for x in raw])

    if not isinstance(raw, dict):
        raise ValueError(
            f"decode: unexpected NexusValue wire type {type(raw).__name__}"
        )

    if len(raw) != 1:
        raise ValueError(
            f"decode: expected single-key tagged NexusValue, got {len(raw)} keys"
        )
    tag, payload = next(iter(raw.items()))
    if tag == "Null":
        return nx.Null()
    if tag == "Bool":
        return nx.Bool(bool(payload))
    if tag == "Int":
        if not isinstance(payload, int):
            raise ValueError("decode: Int payload must be int")
        return nx.Int(payload)
    if tag == "Float":
        if not isinstance(payload, (int, float)):
            raise ValueError("decode: Float payload must be numeric")
        return nx.Float(float(payload))
    if tag == "Bytes":
        if isinstance(payload, (bytes, bytearray, memoryview)):
            return nx.Bytes(bytes(payload))
        if isinstance(payload, list):
            return nx.Bytes(bytes(payload))
        raise ValueError("decode: Bytes payload must be bytes")
    if tag == "Str":
        return nx.Str(str(payload))
    if tag == "Array":
        if not isinstance(payload, list):
            raise ValueError("decode: Array payload must be list")
        return nx.Array([from_wire_value(x) for x in payload])
    if tag == "Map":
        if not isinstance(payload, list):
            raise ValueError("decode: Map payload must be list")
        pairs = []
        for pair in payload:
            if not isinstance(pair, (list, tuple)) or len(pair) != 2:
                raise ValueError("decode: Map entry must be [key, value] pair")
            pairs.append((from_wire_value(pair[0]), from_wire_value(pair[1])))
        return nx.Map(pairs)
    raise ValueError(f"decode: unknown NexusValue tag '{tag}'")


@dataclass
class RpcRequest:
    id: int
    command: str
    args: list


@dataclass
class RpcResponse:
    id: int
    ok: bool
    value: Union[NexusValue, str]  # NexusValue on ok=True, str (error) on ok=False

    def unwrap(self) -> NexusValue:
        if not self.ok:
            raise RuntimeError(f"server: {self.value}")
        assert isinstance(self.value, NexusValue)
        return self.value


def encode_request_frame(req: RpcRequest) -> bytes:
    """Encode a request into a length-prefixed MessagePack frame.

    Wire layout: ``u32_le(body_len) ++ msgpack(body)``.
    """
    body = msgpack.packb(
        {
            "id": req.id,
            "command": req.command,
            "args": [to_wire_value(a) for a in req.args],
        },
        use_bin_type=True,
    )
    return struct.pack("<I", len(body)) + body


def decode_response_body(body: bytes) -> RpcResponse:
    """Decode a response body (MessagePack bytes **after** the length prefix)."""
    raw = msgpack.unpackb(body, raw=False)
    if not isinstance(raw, dict):
        raise ValueError("decode: response must be a map")
    rid = int(raw.get("id", 0))
    result = raw.get("result")
    if not isinstance(result, dict) or len(result) != 1:
        raise ValueError("decode: Result must be a single-key tagged map")
    tag, payload = next(iter(result.items()))
    if tag == _OK_TAG:
        return RpcResponse(id=rid, ok=True, value=from_wire_value(payload))
    if tag == _ERR_TAG:
        return RpcResponse(id=rid, ok=False, value=str(payload))
    raise ValueError(f"decode: Result must be '{_OK_TAG}' or '{_ERR_TAG}', got '{tag}'")


def read_length_prefix(buf: bytes) -> Tuple[int, int]:
    """Return ``(body_len, prefix_len)`` from a buffer head. Raises on short input."""
    if len(buf) < 4:
        raise ValueError("buffer too short for length prefix")
    (body_len,) = struct.unpack("<I", buf[:4])
    return body_len, 4
