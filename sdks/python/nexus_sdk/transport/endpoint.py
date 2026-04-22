"""SDK endpoint URL parsing.

Mirror of ``nexus-cli/src/endpoint.rs`` and
``sdks/rust/src/transport/endpoint.rs`` — same URL grammar so users
can copy-paste endpoints between languages.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Optional, Tuple

RPC_DEFAULT_PORT = 15475
HTTP_DEFAULT_PORT = 15474
HTTPS_DEFAULT_PORT = 443
RESP3_DEFAULT_PORT = 15476


@dataclass(frozen=True)
class Endpoint:
    scheme: str  # 'nexus' | 'http' | 'https' | 'resp3'
    host: str
    port: int

    def authority(self) -> str:
        return f"{self.host}:{self.port}"

    def __str__(self) -> str:
        return f"{self.scheme}://{self.authority()}"

    def as_http_url(self) -> str:
        """Render the endpoint as an HTTP URL (HTTP fallback path).

        Translates ``nexus://`` and ``resp3://`` into the sibling HTTP
        port so the HTTP fallback always has a URL to hit.
        """
        if self.scheme == "http":
            return f"http://{self.authority()}"
        if self.scheme == "https":
            return f"https://{self.authority()}"
        return f"http://{self.host}:{HTTP_DEFAULT_PORT}"

    def is_rpc(self) -> bool:
        return self.scheme == "nexus"


def default_local_endpoint() -> Endpoint:
    """``nexus://127.0.0.1:15475`` — the SDK's default when no URL is given."""
    return Endpoint(scheme="nexus", host="127.0.0.1", port=RPC_DEFAULT_PORT)


def parse_endpoint(raw: str) -> Endpoint:
    """Parse any of the accepted URL forms.

    Raises :class:`ValueError` for unknown schemes, empty input,
    malformed ports, etc. Explicitly rejects ``nexus-rpc://`` so the
    single canonical token stays ``nexus``.
    """
    trimmed = raw.strip()
    if not trimmed:
        raise ValueError("endpoint URL must not be empty")

    if "://" in trimmed:
        scheme_raw, rest = trimmed.split("://", 1)
        rest = rest.rstrip("/")
        scheme = scheme_raw.lower()
        if scheme == "nexus":
            default_port = RPC_DEFAULT_PORT
        elif scheme == "http":
            default_port = HTTP_DEFAULT_PORT
        elif scheme == "https":
            default_port = HTTPS_DEFAULT_PORT
        elif scheme == "resp3":
            default_port = RESP3_DEFAULT_PORT
        else:
            raise ValueError(
                f"unsupported URL scheme '{scheme}://' (expected 'nexus://', 'http://', "
                f"'https://', or 'resp3://')"
            )
        host, port = _split_host_port(rest)
        return Endpoint(
            scheme=scheme, host=host, port=port if port is not None else default_port
        )

    host, port = _split_host_port(trimmed)
    return Endpoint(
        scheme="nexus", host=host, port=port if port is not None else RPC_DEFAULT_PORT
    )


def _split_host_port(s: str) -> Tuple[str, Optional[int]]:
    if not s:
        raise ValueError("missing host")
    if s.startswith("["):
        end = s.find("]")
        if end == -1:
            raise ValueError(f"unterminated IPv6 literal in '{s}'")
        host = s[1:end]
        tail = s[end + 1 :]
        if not tail:
            return host, None
        if not tail.startswith(":"):
            raise ValueError(f"unexpected characters after IPv6 literal: '{tail}'")
        return host, _parse_port(tail[1:])
    if ":" in s:
        host, port_str = s.rsplit(":", 1)
        if not host:
            raise ValueError(f"missing host in '{s}'")
        return host, _parse_port(port_str)
    return s, None


def _parse_port(s: str) -> int:
    try:
        n = int(s)
    except ValueError as e:
        raise ValueError(f"invalid port '{s}': must be 0..=65535") from e
    if n < 0 or n > 65535:
        raise ValueError(f"invalid port '{s}': must be 0..=65535")
    return n
