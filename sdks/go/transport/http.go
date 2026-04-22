package transport

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"time"
)

// HttpTransport wraps a net/http client with the same [Transport]
// interface the RPC path uses. A thin hard-coded route table maps
// wire-level verbs onto the REST endpoints the legacy Go client
// relied on. Unknown verbs surface [ErrUnmappedCommand].
type HttpTransport struct {
	endpoint Endpoint
	creds    Credentials
	baseURL  string
	client   *http.Client
}

// NewHttpTransport builds a fresh HTTP transport.
func NewHttpTransport(endpoint Endpoint, creds Credentials, timeout time.Duration) *HttpTransport {
	if timeout == 0 {
		timeout = 30 * time.Second
	}
	return &HttpTransport{
		endpoint: endpoint,
		creds:    creds,
		baseURL:  endpoint.AsHttpURL(),
		client:   &http.Client{Timeout: timeout},
	}
}

// Execute implements [Transport].
func (t *HttpTransport) Execute(ctx context.Context, req Request) (Response, error) {
	val, err := t.dispatch(ctx, req.Command, req.Args)
	if err != nil {
		return Response{}, err
	}
	return Response{Value: val}, nil
}

// Describe implements [Transport].
func (t *HttpTransport) Describe() string {
	tag := "HTTP"
	if t.endpoint.Scheme == "https" {
		tag = "HTTPS"
	}
	return fmt.Sprintf("%s (%s)", t.endpoint, tag)
}

// IsRpc implements [Transport].
func (t *HttpTransport) IsRpc() bool { return false }

// Close implements [Transport]. net/http has no persistent socket to free.
func (t *HttpTransport) Close() error {
	t.client.CloseIdleConnections()
	return nil
}

func (t *HttpTransport) applyAuth(req *http.Request) {
	if t.creds.APIKey != "" {
		req.Header.Set("X-API-Key", t.creds.APIKey)
	} else if t.creds.Username != "" && t.creds.Password != "" {
		token := base64.StdEncoding.EncodeToString([]byte(t.creds.Username + ":" + t.creds.Password))
		req.Header.Set("Authorization", "Basic "+token)
	}
}

func (t *HttpTransport) dispatch(ctx context.Context, cmd string, args []NexusValue) (NexusValue, error) {
	switch cmd {
	case "CYPHER":
		query, ok := args[0].AsString()
		if !ok {
			return NexusValue{}, fmt.Errorf("HTTP fallback: 'CYPHER' argument 0 must be a string")
		}
		body := map[string]any{"query": query}
		if len(args) > 1 {
			body["parameters"] = NexusToJson(args[1])
		} else {
			body["parameters"] = nil
		}
		return t.doJSON(ctx, http.MethodPost, "/cypher", body)
	case "PING", "HEALTH":
		return t.doJSON(ctx, http.MethodGet, "/health", nil)
	case "STATS":
		return t.doJSON(ctx, http.MethodGet, "/stats", nil)
	case "DB_LIST":
		return t.doJSON(ctx, http.MethodGet, "/databases", nil)
	case "DB_CREATE":
		name, ok := args[0].AsString()
		if !ok {
			return NexusValue{}, fmt.Errorf("HTTP fallback: 'DB_CREATE' argument 0 must be a string")
		}
		return t.doJSON(ctx, http.MethodPost, "/databases", map[string]any{"name": name})
	case "DB_DROP":
		name, ok := args[0].AsString()
		if !ok {
			return NexusValue{}, fmt.Errorf("HTTP fallback: 'DB_DROP' argument 0 must be a string")
		}
		return t.doJSON(ctx, http.MethodDelete, "/databases/"+url.PathEscape(name), nil)
	case "DB_USE":
		name, ok := args[0].AsString()
		if !ok {
			return NexusValue{}, fmt.Errorf("HTTP fallback: 'DB_USE' argument 0 must be a string")
		}
		return t.doJSON(ctx, http.MethodPut, "/session/database", map[string]any{"name": name})
	case "DB_CURRENT":
		return t.doJSON(ctx, http.MethodGet, "/session/database", nil)
	case "LABELS":
		return t.doJSON(ctx, http.MethodGet, "/schema/labels", nil)
	case "REL_TYPES":
		return t.doJSON(ctx, http.MethodGet, "/schema/relationship-types", nil)
	case "EXPORT":
		fmtStr, ok := args[0].AsString()
		if !ok {
			return NexusValue{}, fmt.Errorf("HTTP fallback: 'EXPORT' argument 0 must be a string")
		}
		text, err := t.doText(ctx, http.MethodGet, "/export?format="+url.QueryEscape(fmtStr), nil, "")
		if err != nil {
			return NexusValue{}, err
		}
		return JsonToNexus(map[string]any{"format": fmtStr, "data": text}), nil
	case "IMPORT":
		fmtStr, ok := args[0].AsString()
		if !ok {
			return NexusValue{}, fmt.Errorf("HTTP fallback: 'IMPORT' argument 0 must be a string")
		}
		payload, ok := args[1].AsString()
		if !ok {
			return NexusValue{}, fmt.Errorf("HTTP fallback: 'IMPORT' argument 1 must be a string")
		}
		return t.doRaw(ctx, http.MethodPost, "/import?format="+url.QueryEscape(fmtStr), payload, "text/plain")
	}
	return NexusValue{}, &ErrUnmappedCommand{Command: cmd}
}

func (t *HttpTransport) doJSON(ctx context.Context, method, path string, body any) (NexusValue, error) {
	var reqBody io.Reader
	if body != nil {
		data, err := json.Marshal(body)
		if err != nil {
			return NexusValue{}, err
		}
		reqBody = bytes.NewReader(data)
	}
	req, err := http.NewRequestWithContext(ctx, method, t.baseURL+path, reqBody)
	if err != nil {
		return NexusValue{}, err
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	t.applyAuth(req)
	resp, err := t.client.Do(req)
	if err != nil {
		return NexusValue{}, err
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return NexusValue{}, err
	}
	if resp.StatusCode >= 400 {
		return NexusValue{}, &HttpError{StatusCode: resp.StatusCode, Body: string(raw)}
	}
	if len(raw) == 0 {
		return NxNull(), nil
	}
	var decoded any
	if err := json.Unmarshal(raw, &decoded); err != nil {
		// Fall back to string if JSON decode fails.
		return NxStr(string(raw)), nil
	}
	return JsonToNexus(decoded), nil
}

func (t *HttpTransport) doText(ctx context.Context, method, path string, body io.Reader, contentType string) (string, error) {
	req, err := http.NewRequestWithContext(ctx, method, t.baseURL+path, body)
	if err != nil {
		return "", err
	}
	if contentType != "" {
		req.Header.Set("Content-Type", contentType)
	}
	t.applyAuth(req)
	resp, err := t.client.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", err
	}
	if resp.StatusCode >= 400 {
		return "", &HttpError{StatusCode: resp.StatusCode, Body: string(raw)}
	}
	return string(raw), nil
}

func (t *HttpTransport) doRaw(ctx context.Context, method, path, body, contentType string) (NexusValue, error) {
	req, err := http.NewRequestWithContext(ctx, method, t.baseURL+path, bytes.NewBufferString(body))
	if err != nil {
		return NexusValue{}, err
	}
	req.Header.Set("Content-Type", contentType)
	t.applyAuth(req)
	resp, err := t.client.Do(req)
	if err != nil {
		return NexusValue{}, err
	}
	defer resp.Body.Close()
	raw, err := io.ReadAll(resp.Body)
	if err != nil {
		return NexusValue{}, err
	}
	if resp.StatusCode >= 400 {
		return NexusValue{}, &HttpError{StatusCode: resp.StatusCode, Body: string(raw)}
	}
	if len(raw) == 0 {
		return NxNull(), nil
	}
	var decoded any
	if err := json.Unmarshal(raw, &decoded); err != nil {
		return NxStr(string(raw)), nil
	}
	return JsonToNexus(decoded), nil
}
