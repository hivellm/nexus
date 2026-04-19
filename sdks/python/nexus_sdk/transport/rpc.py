"""Native binary RPC transport — asyncio implementation.

Holds a single TCP stream per client (frames cannot interleave) and
serialises concurrent ``execute()`` calls behind a single writer lock.
Mirrors the Rust and TypeScript SDK transports: lazy connect, HELLO
handshake, optional AUTH, monotonic request ids skipping ``PUSH_ID``.
"""

from __future__ import annotations

import asyncio
import struct
from typing import Dict, List, Optional

from nexus_sdk.transport.codec import (
    RpcRequest,
    RpcResponse,
    decode_response_body,
    encode_request_frame,
)
from nexus_sdk.transport.endpoint import Endpoint
from nexus_sdk.transport.types import (
    NexusValue,
    Transport,
    TransportCredentials,
    TransportRequest,
    TransportResponse,
    nx,
)


# Reserved id used by the server for PUSH frames — clients must skip it.
PUSH_ID = 0xFFFFFFFF


class RpcTransport(Transport):
    """Single-socket RPC client.

    One TCP stream is opened lazily on the first request. Outgoing
    frames go through ``_write_lock`` so writes never interleave; the
    reader task multiplexes responses back to pending futures keyed
    by request id.
    """

    def __init__(
        self,
        endpoint: Endpoint,
        credentials: TransportCredentials,
        connect_timeout_s: float = 5.0,
    ) -> None:
        self._endpoint = endpoint
        self._credentials = credentials
        self._connect_timeout_s = connect_timeout_s

        self._reader: Optional[asyncio.StreamReader] = None
        self._writer: Optional[asyncio.StreamWriter] = None
        self._reader_task: Optional[asyncio.Task[None]] = None
        self._write_lock = asyncio.Lock()
        self._connect_lock = asyncio.Lock()
        self._pending: Dict[int, asyncio.Future[RpcResponse]] = {}
        self._next_id = 1

    async def execute(self, req: TransportRequest) -> TransportResponse:
        resp = await self.call(req.command, req.args)
        return TransportResponse(value=resp.unwrap())

    def describe(self) -> str:
        return f"{self._endpoint} (RPC)"

    def is_rpc(self) -> bool:
        return True

    async def close(self) -> None:
        if self._reader_task and not self._reader_task.done():
            self._reader_task.cancel()
            try:
                await self._reader_task
            except (asyncio.CancelledError, Exception):
                pass
        if self._writer is not None:
            self._writer.close()
            try:
                await self._writer.wait_closed()
            except Exception:
                pass
        self._reader = None
        self._writer = None
        self._reader_task = None
        for fut in self._pending.values():
            if not fut.done():
                fut.set_exception(ConnectionError("RPC transport closed"))
        self._pending.clear()

    async def call(self, command: str, args: List[NexusValue]) -> RpcResponse:
        """Low-level single request. Lazy-connects on first use."""
        await self._ensure_connected()
        return await self._send(RpcRequest(id=self._alloc_id(), command=command, args=args))

    # ── Internals ──────────────────────────────────────────────────────

    def _alloc_id(self) -> int:
        nid = self._next_id
        self._next_id += 1
        if nid == PUSH_ID:
            nid = self._next_id
            self._next_id += 1
        if self._next_id >= 0xFFFFFFFE:
            self._next_id = 1
        return nid

    async def _ensure_connected(self) -> None:
        if self._writer is not None and not self._writer.is_closing():
            return
        async with self._connect_lock:
            if self._writer is not None and not self._writer.is_closing():
                return
            await self._connect()

    async def _connect(self) -> None:
        authority = self._endpoint.authority()
        try:
            reader, writer = await asyncio.wait_for(
                asyncio.open_connection(self._endpoint.host, self._endpoint.port),
                timeout=self._connect_timeout_s,
            )
        except (asyncio.TimeoutError, OSError) as e:
            raise ConnectionError(f"failed to connect to {authority}: {e}") from e

        # Disable Nagle so small frames land promptly.
        try:
            sock = writer.get_extra_info("socket")
            if sock is not None:
                import socket

                sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        except Exception:
            pass

        self._reader = reader
        self._writer = writer
        self._reader_task = asyncio.create_task(self._read_loop(reader))

        # HELLO + AUTH handshake.
        hello = await self._send(
            RpcRequest(id=0, command="HELLO", args=[nx.Int(1)])
        )
        if not hello.ok:
            raise ConnectionError(f"HELLO rejected by server: {hello.value}")

        if self._credentials.has_any():
            if self._credentials.api_key:
                args = [nx.Str(self._credentials.api_key)]
            else:
                args = [
                    nx.Str(self._credentials.username or ""),
                    nx.Str(self._credentials.password or ""),
                ]
            auth = await self._send(RpcRequest(id=0, command="AUTH", args=args))
            if not auth.ok:
                raise ConnectionError(f"authentication failed: {auth.value}")

    async def _send(self, req: RpcRequest) -> RpcResponse:
        writer = self._writer
        if writer is None or writer.is_closing():
            raise ConnectionError("RPC transport is not connected")
        loop = asyncio.get_running_loop()
        fut: asyncio.Future[RpcResponse] = loop.create_future()
        self._pending[req.id] = fut
        frame = encode_request_frame(req)
        try:
            async with self._write_lock:
                writer.write(frame)
                await writer.drain()
        except Exception as e:
            self._pending.pop(req.id, None)
            raise ConnectionError(f"failed to send RPC frame: {e}") from e
        return await fut

    async def _read_loop(self, reader: asyncio.StreamReader) -> None:
        try:
            while True:
                prefix = await reader.readexactly(4)
                (length,) = struct.unpack("<I", prefix)
                body = await reader.readexactly(length)
                try:
                    resp = decode_response_body(body)
                except Exception as e:
                    self._fail_all(ConnectionError(f"malformed RPC frame: {e}"))
                    return
                fut = self._pending.pop(resp.id, None)
                if fut is not None and not fut.done():
                    fut.set_result(resp)
                # Unknown ids (including PUSH_ID) are dropped — push
                # subscriptions are not wired up on the SDK side yet.
        except asyncio.IncompleteReadError:
            self._fail_all(ConnectionError("RPC connection closed"))
        except asyncio.CancelledError:
            # Normal shutdown.
            pass
        except Exception as e:
            self._fail_all(ConnectionError(f"RPC socket error: {e}"))

    def _fail_all(self, exc: BaseException) -> None:
        for fut in self._pending.values():
            if not fut.done():
                fut.set_exception(exc)
        self._pending.clear()
