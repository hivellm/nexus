package transport

import (
	"fmt"
	"strconv"
	"strings"
)

// Default ports — see docs/specs/sdk-transport.md §3.
const (
	RpcDefaultPort   uint16 = 15475
	HttpDefaultPort  uint16 = 15474
	HttpsDefaultPort uint16 = 443
	Resp3DefaultPort uint16 = 15476
)

// Endpoint is a parsed URL.
type Endpoint struct {
	Scheme string // "nexus" | "http" | "https" | "resp3"
	Host   string
	Port   uint16
}

// DefaultLocalEndpoint returns `nexus://127.0.0.1:15475`.
func DefaultLocalEndpoint() Endpoint {
	return Endpoint{Scheme: "nexus", Host: "127.0.0.1", Port: RpcDefaultPort}
}

// Authority returns "host:port".
func (e Endpoint) Authority() string {
	return fmt.Sprintf("%s:%d", e.Host, e.Port)
}

// String renders the endpoint as a URL.
func (e Endpoint) String() string {
	return fmt.Sprintf("%s://%s", e.Scheme, e.Authority())
}

// IsRpc reports whether this endpoint names the RPC scheme.
func (e Endpoint) IsRpc() bool { return e.Scheme == "nexus" }

// AsHttpURL renders the endpoint as an HTTP URL. `nexus://` and
// `resp3://` schemes swap to the sibling HTTP port (15474).
func (e Endpoint) AsHttpURL() string {
	switch e.Scheme {
	case "http":
		return "http://" + e.Authority()
	case "https":
		return "https://" + e.Authority()
	default:
		return fmt.Sprintf("http://%s:%d", e.Host, HttpDefaultPort)
	}
}

// ParseEndpoint parses the accepted URL forms. Rejects `nexus-rpc://`
// explicitly — the single canonical token is `nexus`.
func ParseEndpoint(raw string) (Endpoint, error) {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return Endpoint{}, fmt.Errorf("endpoint URL must not be empty")
	}

	if idx := strings.Index(trimmed, "://"); idx != -1 {
		schemeRaw := strings.ToLower(trimmed[:idx])
		rest := strings.TrimRight(trimmed[idx+3:], "/")
		var scheme string
		var defaultPort uint16
		switch schemeRaw {
		case "nexus":
			scheme, defaultPort = "nexus", RpcDefaultPort
		case "http":
			scheme, defaultPort = "http", HttpDefaultPort
		case "https":
			scheme, defaultPort = "https", HttpsDefaultPort
		case "resp3":
			scheme, defaultPort = "resp3", Resp3DefaultPort
		default:
			return Endpoint{}, fmt.Errorf(
				"unsupported URL scheme '%s://' (expected 'nexus://', 'http://', 'https://', or 'resp3://')",
				schemeRaw,
			)
		}
		host, port, err := splitHostPort(rest)
		if err != nil {
			return Endpoint{}, err
		}
		if port == 0 {
			port = defaultPort
		}
		return Endpoint{Scheme: scheme, Host: host, Port: port}, nil
	}

	// Bare form: host[:port] → treat as RPC.
	host, port, err := splitHostPort(trimmed)
	if err != nil {
		return Endpoint{}, err
	}
	if port == 0 {
		port = RpcDefaultPort
	}
	return Endpoint{Scheme: "nexus", Host: host, Port: port}, nil
}

func splitHostPort(s string) (string, uint16, error) {
	if s == "" {
		return "", 0, fmt.Errorf("missing host")
	}
	if strings.HasPrefix(s, "[") {
		end := strings.Index(s, "]")
		if end == -1 {
			return "", 0, fmt.Errorf("unterminated IPv6 literal in '%s'", s)
		}
		host := s[1:end]
		tail := s[end+1:]
		if tail == "" {
			return host, 0, nil
		}
		if !strings.HasPrefix(tail, ":") {
			return "", 0, fmt.Errorf("unexpected characters after IPv6 literal: '%s'", tail)
		}
		port, err := parsePort(tail[1:])
		if err != nil {
			return "", 0, err
		}
		return host, port, nil
	}
	if idx := strings.LastIndex(s, ":"); idx != -1 {
		host := s[:idx]
		if host == "" {
			return "", 0, fmt.Errorf("missing host in '%s'", s)
		}
		port, err := parsePort(s[idx+1:])
		if err != nil {
			return "", 0, err
		}
		return host, port, nil
	}
	return s, 0, nil
}

func parsePort(s string) (uint16, error) {
	n, err := strconv.Atoi(s)
	if err != nil || n < 0 || n > 65535 {
		return 0, fmt.Errorf("invalid port '%s': must be 0..=65535", s)
	}
	return uint16(n), nil
}
