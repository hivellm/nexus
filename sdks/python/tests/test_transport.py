"""Transport-layer unit tests — Python SDK."""

from __future__ import annotations

import struct

import msgpack  # type: ignore[import-untyped]
import pytest

from nexus_sdk.transport.codec import (
    RpcRequest,
    decode_response_body,
    encode_request_frame,
    from_wire_value,
    to_wire_value,
)
from nexus_sdk.transport.command_map import (
    json_to_nexus,
    map_command,
    nexus_to_json,
)
from nexus_sdk.transport.endpoint import (
    HTTP_DEFAULT_PORT,
    HTTPS_DEFAULT_PORT,
    RPC_DEFAULT_PORT,
    Endpoint,
    default_local_endpoint,
    parse_endpoint,
)
from nexus_sdk.transport.factory import build_transport
from nexus_sdk.transport.rpc import RpcTransport
from nexus_sdk.transport.types import TransportCredentials, TransportMode, nx

# ── Endpoint parser ────────────────────────────────────────────────────


class TestEndpoint:
    def test_default_local_is_nexus_loopback(self) -> None:
        ep = default_local_endpoint()
        assert ep.scheme == "nexus"
        assert ep.host == "127.0.0.1"
        assert ep.port == RPC_DEFAULT_PORT
        assert str(ep) == "nexus://127.0.0.1:15475"

    def test_parse_nexus_with_explicit_port(self) -> None:
        ep = parse_endpoint("nexus://example.com:17000")
        assert ep.scheme == "nexus"
        assert ep.host == "example.com"
        assert ep.port == 17000

    def test_parse_nexus_default_port(self) -> None:
        ep = parse_endpoint("nexus://db.internal")
        assert ep.port == RPC_DEFAULT_PORT

    def test_parse_http_default_port(self) -> None:
        ep = parse_endpoint("http://localhost")
        assert ep.scheme == "http"
        assert ep.port == HTTP_DEFAULT_PORT

    def test_parse_https_default_port(self) -> None:
        ep = parse_endpoint("https://nexus.example.com")
        assert ep.scheme == "https"
        assert ep.port == HTTPS_DEFAULT_PORT

    def test_parse_bare_form_is_rpc(self) -> None:
        ep = parse_endpoint("10.0.0.5:15600")
        assert ep.scheme == "nexus"
        assert ep.port == 15600

    def test_parse_ipv6_with_port(self) -> None:
        ep = parse_endpoint("nexus://[::1]:15475")
        assert ep.host == "::1"
        assert ep.port == 15475

    def test_rejects_nexus_rpc_scheme(self) -> None:
        with pytest.raises(ValueError, match="unsupported URL scheme"):
            parse_endpoint("nexus-rpc://host")

    def test_rejects_empty(self) -> None:
        with pytest.raises(ValueError):
            parse_endpoint("")
        with pytest.raises(ValueError):
            parse_endpoint("   ")

    def test_as_http_url_swaps_rpc_to_sibling_http_port(self) -> None:
        ep = parse_endpoint("nexus://host:17000")
        assert ep.as_http_url() == "http://host:15474"


# ── Wire codec: NexusValue ────────────────────────────────────────────


class TestWireValue:
    def test_encodes_null_as_literal_string(self) -> None:
        assert to_wire_value(nx.Null()) == "Null"

    def test_encodes_str_as_tagged_map(self) -> None:
        assert to_wire_value(nx.Str("hi")) == {"Str": "hi"}

    def test_encodes_primitives(self) -> None:
        assert to_wire_value(nx.Bool(True)) == {"Bool": True}
        assert to_wire_value(nx.Int(42)) == {"Int": 42}
        assert to_wire_value(nx.Float(1.5)) == {"Float": 1.5}
        assert to_wire_value(nx.Bytes(b"\x01\x02")) == {"Bytes": b"\x01\x02"}

    def test_roundtrips_primitive_variants(self) -> None:
        cases = [
            nx.Null(),
            nx.Bool(False),
            nx.Bool(True),
            nx.Int(0),
            nx.Int(-1),
            nx.Str(""),
            nx.Str("hello"),
            nx.Float(3.14),
            nx.Bytes(b"\x00\xff"),
        ]
        for v in cases:
            assert from_wire_value(to_wire_value(v)) == v

    def test_roundtrips_nested_array_and_map(self) -> None:
        v = nx.Map(
            [
                (nx.Str("labels"), nx.Array([nx.Str("Person")])),
                (nx.Str("age"), nx.Int(30)),
            ]
        )
        assert from_wire_value(to_wire_value(v)) == v

    def test_rejects_multi_key_tagged_value(self) -> None:
        with pytest.raises(ValueError, match="single-key"):
            from_wire_value({"Str": "a", "Int": 1})

    def test_rejects_unknown_tag(self) -> None:
        with pytest.raises(ValueError, match="unknown NexusValue tag"):
            from_wire_value({"Widget": "x"})


class TestFrameCodec:
    def test_frame_has_u32_le_length_prefix(self) -> None:
        frame = encode_request_frame(RpcRequest(id=7, command="PING", args=[]))
        (length,) = struct.unpack("<I", frame[:4])
        assert length == len(frame) - 4
        assert length > 0

    def test_decodes_ok_response(self) -> None:
        body = msgpack.packb(
            {"id": 9, "result": {"Ok": {"Str": "OK"}}}, use_bin_type=True
        )
        resp = decode_response_body(body)
        assert resp.id == 9
        assert resp.ok is True
        assert resp.unwrap() == nx.Str("OK")

    def test_decodes_err_response(self) -> None:
        body = msgpack.packb({"id": 3, "result": {"Err": "boom"}}, use_bin_type=True)
        resp = decode_response_body(body)
        assert resp.ok is False
        assert resp.value == "boom"
        with pytest.raises(RuntimeError, match="boom"):
            resp.unwrap()


# ── Command map ───────────────────────────────────────────────────────


class TestCommandMap:
    def test_cypher_simple_query(self) -> None:
        m = map_command("graph.cypher", {"query": "RETURN 1"})
        assert m is not None
        assert m.command == "CYPHER"
        assert m.args == [nx.Str("RETURN 1")]

    def test_cypher_with_parameters(self) -> None:
        m = map_command(
            "graph.cypher",
            {"query": "MATCH (n {name:$n}) RETURN n", "parameters": {"n": "Alice"}},
        )
        assert m is not None
        assert len(m.args) == 2
        assert m.args[1].kind == "Map"

    def test_no_arg_verbs(self) -> None:
        for name in ("graph.ping", "graph.stats", "graph.health", "graph.quit"):
            m = map_command(name, {})
            assert m is not None
            assert m.args == []

    def test_auth_api_key_wins_over_user_pass(self) -> None:
        m = map_command(
            "auth.login", {"api_key": "nx_1", "username": "u", "password": "p"}
        )
        assert m is not None
        assert m.args == [nx.Str("nx_1")]

    def test_auth_falls_back_to_user_pass(self) -> None:
        m = map_command("auth.login", {"username": "u", "password": "p"})
        assert m is not None
        assert len(m.args) == 2

    def test_db_create_requires_name(self) -> None:
        assert map_command("db.create", {}) is None
        m = map_command("db.create", {"name": "mydb"})
        assert m is not None
        assert m.command == "DB_CREATE"

    def test_export_with_and_without_query(self) -> None:
        m1 = map_command("data.export", {"format": "json"})
        assert m1 is not None
        assert len(m1.args) == 1
        m2 = map_command(
            "data.export", {"format": "csv", "query": "MATCH (n) RETURN n"}
        )
        assert m2 is not None
        assert len(m2.args) == 2

    def test_import_requires_format_and_data(self) -> None:
        assert map_command("data.import", {"format": "json"}) is None
        assert map_command("data.import", {"data": "[]"}) is None
        m = map_command("data.import", {"format": "json", "data": "[]"})
        assert m is not None
        assert m.command == "IMPORT"

    def test_unknown_dotted_name(self) -> None:
        assert map_command("graph.nonsense", {}) is None

    def test_json_to_nexus_nested(self) -> None:
        v = json_to_nexus(
            {"labels": ["Person"], "properties": {"name": "Alice", "age": 30}}
        )
        assert v.kind == "Map"

    def test_nexus_to_json_roundtrips_map(self) -> None:
        v = nx.Map([(nx.Str("name"), nx.Str("Alice")), (nx.Str("age"), nx.Int(30))])
        assert nexus_to_json(v) == {"name": "Alice", "age": 30}


# ── TransportMode parse ───────────────────────────────────────────────


class TestTransportMode:
    def test_canonical_tokens(self) -> None:
        assert TransportMode.parse("nexus") is TransportMode.NEXUS
        assert TransportMode.parse("http") is TransportMode.HTTP
        assert TransportMode.parse("https") is TransportMode.HTTPS
        assert TransportMode.parse("resp3") is TransportMode.RESP3

    def test_aliases(self) -> None:
        assert TransportMode.parse("rpc") is TransportMode.NEXUS
        assert TransportMode.parse("NexusRpc") is TransportMode.NEXUS

    def test_auto_and_empty_return_none(self) -> None:
        assert TransportMode.parse("") is None
        assert TransportMode.parse("auto") is None
        assert TransportMode.parse("widget") is None


# ── build_transport precedence ────────────────────────────────────────


class TestBuildTransportPrecedence:
    def test_default_is_rpc(self) -> None:
        built = build_transport(None, TransportCredentials(), env_transport="")
        assert built.mode is TransportMode.NEXUS
        assert built.endpoint.port == RPC_DEFAULT_PORT

    def test_url_scheme_wins_over_env_var(self) -> None:
        built = build_transport(
            "http://host:15474", TransportCredentials(), env_transport="nexus"
        )
        assert built.mode is TransportMode.HTTP

    def test_env_var_overrides_bare_host(self) -> None:
        built = build_transport(
            "host:15474", TransportCredentials(), env_transport="http"
        )
        assert built.mode is TransportMode.HTTP

    def test_transport_hint_honoured_when_bare(self) -> None:
        built = build_transport(
            "host:15474",
            TransportCredentials(),
            transport_hint=TransportMode.HTTP,
            env_transport="",
        )
        assert built.mode is TransportMode.HTTP

    def test_resp3_raises_clear_error(self) -> None:
        with pytest.raises(ValueError, match="resp3 transport is not yet shipped"):
            build_transport(
                None,
                TransportCredentials(),
                transport_hint=TransportMode.RESP3,
                env_transport="",
            )


# ── RpcTransport fails-fast path ──────────────────────────────────────


class TestRpcTransportFailFast:
    @pytest.mark.asyncio
    async def test_call_fails_fast_on_unreachable_host(self) -> None:
        ep = Endpoint(scheme="nexus", host="127.0.0.1", port=1)  # port 1 is reserved
        t = RpcTransport(ep, TransportCredentials(), connect_timeout_s=0.5)
        with pytest.raises(ConnectionError, match="failed to connect"):
            await t.call("PING", [])
        await t.close()


# ── RpcCredentials.has_any ────────────────────────────────────────────


class TestCredentials:
    def test_empty_has_none(self) -> None:
        assert not TransportCredentials().has_any()

    def test_api_key_sets_flag(self) -> None:
        assert TransportCredentials(api_key="k").has_any()

    def test_username_alone_does_not_count(self) -> None:
        assert not TransportCredentials(username="u").has_any()

    def test_user_and_pass_together_count(self) -> None:
        assert TransportCredentials(username="u", password="p").has_any()
