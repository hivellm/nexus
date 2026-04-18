// Package nexus provides a Go client for the Nexus graph database.
package nexus

import (
	"context"
	"math"
	"math/rand"
	"net/http"
	"time"
)

// RetryConfig holds configuration for retry behavior.
type RetryConfig struct {
	// MaxRetries is the maximum number of retry attempts (default: 3)
	MaxRetries int
	// InitialBackoff is the initial backoff duration (default: 100ms)
	InitialBackoff time.Duration
	// MaxBackoff is the maximum backoff duration (default: 10s)
	MaxBackoff time.Duration
	// BackoffMultiplier is the multiplier for exponential backoff (default: 2.0)
	BackoffMultiplier float64
	// Jitter adds randomness to backoff to prevent thundering herd (default: true)
	Jitter bool
	// RetryableStatusCodes defines which HTTP status codes should trigger a retry
	RetryableStatusCodes []int
}

// DefaultRetryConfig returns a RetryConfig with sensible defaults.
func DefaultRetryConfig() *RetryConfig {
	return &RetryConfig{
		MaxRetries:        3,
		InitialBackoff:    100 * time.Millisecond,
		MaxBackoff:        10 * time.Second,
		BackoffMultiplier: 2.0,
		Jitter:            true,
		RetryableStatusCodes: []int{
			http.StatusRequestTimeout,      // 408
			http.StatusTooManyRequests,     // 429
			http.StatusInternalServerError, // 500
			http.StatusBadGateway,          // 502
			http.StatusServiceUnavailable,  // 503
			http.StatusGatewayTimeout,      // 504
		},
	}
}

// isRetryableError checks if an error is retryable based on the config.
func (c *RetryConfig) isRetryableError(err error) bool {
	if err == nil {
		return false
	}

	// Check if it's a Nexus API error with a retryable status code
	if apiErr, ok := err.(*Error); ok {
		for _, code := range c.RetryableStatusCodes {
			if apiErr.StatusCode == code {
				return true
			}
		}
		return false
	}

	// For other errors (network errors, timeouts), retry by default
	return true
}

// calculateBackoff returns the backoff duration for a given attempt.
func (c *RetryConfig) calculateBackoff(attempt int) time.Duration {
	backoff := float64(c.InitialBackoff) * math.Pow(c.BackoffMultiplier, float64(attempt))

	if c.Jitter {
		// Add ±25% jitter
		jitterRange := backoff * 0.25
		backoff = backoff - jitterRange + (rand.Float64() * jitterRange * 2)
	}

	duration := time.Duration(backoff)
	if duration > c.MaxBackoff {
		duration = c.MaxBackoff
	}

	return duration
}

// RetryableClient wraps a Client with retry functionality.
type RetryableClient struct {
	*Client
	retryConfig *RetryConfig
}

// NewRetryableClient creates a new client with retry support.
func NewRetryableClient(config Config, retryConfig *RetryConfig) *RetryableClient {
	if retryConfig == nil {
		retryConfig = DefaultRetryConfig()
	}

	return &RetryableClient{
		Client:      NewClient(config),
		retryConfig: retryConfig,
	}
}

// WithRetry adds retry capability to an existing client.
func (c *Client) WithRetry(retryConfig *RetryConfig) *RetryableClient {
	if retryConfig == nil {
		retryConfig = DefaultRetryConfig()
	}

	return &RetryableClient{
		Client:      c,
		retryConfig: retryConfig,
	}
}

// doRequestWithRetry performs an HTTP request with automatic retry on failure.
func (rc *RetryableClient) doRequestWithRetry(ctx context.Context, method, path string, body interface{}) (*http.Response, error) {
	var lastErr error

	for attempt := 0; attempt <= rc.retryConfig.MaxRetries; attempt++ {
		// Check context cancellation before each attempt
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		default:
		}

		// Make the request
		resp, err := rc.Client.doRequest(ctx, method, path, body)
		if err == nil {
			return resp, nil
		}

		lastErr = err

		// Check if we should retry
		if !rc.retryConfig.isRetryableError(err) {
			return nil, err
		}

		// Don't sleep after the last attempt
		if attempt < rc.retryConfig.MaxRetries {
			backoff := rc.retryConfig.calculateBackoff(attempt)

			select {
			case <-ctx.Done():
				return nil, ctx.Err()
			case <-time.After(backoff):
				// Continue to next attempt
			}
		}
	}

	return nil, lastErr
}

// ExecuteCypher executes a Cypher query with automatic retry.
func (rc *RetryableClient) ExecuteCypher(ctx context.Context, query string, params map[string]interface{}) (*QueryResult, error) {
	reqBody := map[string]interface{}{
		"query": query,
	}
	if params != nil {
		reqBody["parameters"] = params
	}

	resp, err := rc.doRequestWithRetry(ctx, http.MethodPost, "/cypher", reqBody)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var result QueryResult
	if err := decodeResponse(resp, &result); err != nil {
		return nil, err
	}

	return &result, nil
}

// Ping checks if the server is reachable with automatic retry.
func (rc *RetryableClient) Ping(ctx context.Context) error {
	resp, err := rc.doRequestWithRetry(ctx, http.MethodGet, "/health", nil)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	return nil
}

// CreateNode creates a new node with automatic retry.
func (rc *RetryableClient) CreateNode(ctx context.Context, labels []string, properties map[string]interface{}) (*Node, error) {
	reqBody := map[string]interface{}{
		"labels":     labels,
		"properties": properties,
	}

	resp, err := rc.doRequestWithRetry(ctx, http.MethodPost, "/nodes", reqBody)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var node Node
	if err := decodeResponse(resp, &node); err != nil {
		return nil, err
	}

	return &node, nil
}

// GetNode retrieves a node by its ID with automatic retry.
func (rc *RetryableClient) GetNode(ctx context.Context, id string) (*Node, error) {
	resp, err := rc.doRequestWithRetry(ctx, http.MethodGet, "/nodes/"+id, nil)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var node Node
	if err := decodeResponse(resp, &node); err != nil {
		return nil, err
	}

	return &node, nil
}

// ListLabels retrieves all node labels with automatic retry.
func (rc *RetryableClient) ListLabels(ctx context.Context) ([]string, error) {
	resp, err := rc.doRequestWithRetry(ctx, http.MethodGet, "/schema/labels", nil)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var result struct {
		Labels []string `json:"labels"`
	}
	if err := decodeResponse(resp, &result); err != nil {
		return nil, err
	}

	return result.Labels, nil
}

// ListRelationshipTypes retrieves all relationship types with automatic retry.
func (rc *RetryableClient) ListRelationshipTypes(ctx context.Context) ([]string, error) {
	resp, err := rc.doRequestWithRetry(ctx, http.MethodGet, "/schema/relationship-types", nil)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var result struct {
		Types []string `json:"types"`
	}
	if err := decodeResponse(resp, &result); err != nil {
		return nil, err
	}

	return result.Types, nil
}
