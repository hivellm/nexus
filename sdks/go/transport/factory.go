package transport

import (
	"fmt"
	"os"
	"strings"
	"time"
)

// BuildOptions controls how [Build] picks a transport.
type BuildOptions struct {
	// BaseURL — endpoint URL (`nexus://host:15475`, `http://host:15474`, …).
	// Defaults to `nexus://127.0.0.1:15475` when empty.
	BaseURL string
	// Transport is the explicit mode hint. URL scheme wins if set.
	Transport Mode
	// RpcPort overrides the default RPC port (15475).
	RpcPort uint16
	// Resp3Port overrides the default RESP3 port (15476).
	Resp3Port uint16
	// Timeout — HTTP request timeout (ignored by RPC).
	Timeout time.Duration
	// EnvTransport — injected test shim for NEXUS_SDK_TRANSPORT. Leave
	// empty to read from os.Environ.
	EnvTransport string
}

// Built is the resolved-transport tuple.
type Built struct {
	Transport Transport
	Endpoint  Endpoint
	Mode      Mode
}

// Build applies the precedence chain and returns a fresh transport.
//
// Precedence (highest wins):
//  1. URL scheme in BaseURL (`nexus://` → RPC, `http://` → HTTP, …)
//  2. NEXUS_SDK_TRANSPORT env var
//  3. BuildOptions.Transport field
//  4. Default: ModeNexusRpc
func Build(opts BuildOptions, creds Credentials) (Built, error) {
	var endpoint Endpoint
	if opts.BaseURL != "" {
		ep, err := ParseEndpoint(opts.BaseURL)
		if err != nil {
			return Built{}, err
		}
		endpoint = ep
	} else {
		endpoint = DefaultLocalEndpoint()
	}

	// 1. URL scheme wins.
	mode := schemeToMode(endpoint.Scheme)

	// 2. Env var overrides a bare URL (no scheme).
	explicitScheme := opts.BaseURL != "" && strings.Contains(opts.BaseURL, "://")
	envRaw := opts.EnvTransport
	if envRaw == "" {
		envRaw = os.Getenv("NEXUS_SDK_TRANSPORT")
	}
	if envMode, ok := ParseMode(envRaw); ok && !explicitScheme {
		mode = envMode
		endpoint = realignEndpoint(endpoint, mode, opts)
	}

	// 3. Config hint.
	if opts.Transport != "" && !explicitScheme && envRaw == "" {
		if _, ok := ParseMode(string(opts.Transport)); ok {
			mode = opts.Transport
			endpoint = realignEndpoint(endpoint, mode, opts)
		}
	}

	switch mode {
	case ModeNexusRpc:
		return Built{
			Transport: NewRpcTransport(endpoint, creds),
			Endpoint:  endpoint,
			Mode:      mode,
		}, nil
	case ModeHttp, ModeHttps:
		return Built{
			Transport: NewHttpTransport(endpoint, creds, opts.Timeout),
			Endpoint:  endpoint,
			Mode:      mode,
		}, nil
	case ModeResp3:
		return Built{}, fmt.Errorf(
			"resp3 transport is not yet shipped in the Go SDK — use 'nexus' (RPC) or 'http' for now",
		)
	}
	return Built{}, fmt.Errorf("unknown transport mode: %s", mode)
}

func schemeToMode(scheme string) Mode {
	switch scheme {
	case "nexus":
		return ModeNexusRpc
	case "resp3":
		return ModeResp3
	case "https":
		return ModeHttps
	default:
		return ModeHttp
	}
}

func realignEndpoint(ep Endpoint, mode Mode, opts BuildOptions) Endpoint {
	switch mode {
	case ModeNexusRpc:
		port := opts.RpcPort
		if port == 0 {
			port = RpcDefaultPort
		}
		return Endpoint{Scheme: "nexus", Host: ep.Host, Port: port}
	case ModeResp3:
		port := opts.Resp3Port
		if port == 0 {
			port = Resp3DefaultPort
		}
		return Endpoint{Scheme: "resp3", Host: ep.Host, Port: port}
	case ModeHttps:
		return Endpoint{Scheme: "https", Host: ep.Host, Port: HttpsDefaultPort}
	}
	return Endpoint{Scheme: "http", Host: ep.Host, Port: HttpDefaultPort}
}
