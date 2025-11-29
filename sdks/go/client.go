// Package nexus provides a Go client for the Nexus graph database.
package nexus

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"time"
)

// Client represents a Nexus database client.
type Client struct {
	baseURL    string
	httpClient *http.Client
	apiKey     string
	username   string
	password   string
	token      string
}

// Config holds configuration options for the Nexus client.
type Config struct {
	BaseURL  string
	APIKey   string
	Username string
	Password string
	Timeout  time.Duration
}

// NewClient creates a new Nexus client with the given configuration.
func NewClient(config Config) *Client {
	if config.Timeout == 0 {
		config.Timeout = 30 * time.Second
	}

	return &Client{
		baseURL: config.BaseURL,
		httpClient: &http.Client{
			Timeout: config.Timeout,
		},
		apiKey:   config.APIKey,
		username: config.Username,
		password: config.Password,
	}
}

// QueryResult represents the result of a Cypher query.
type QueryResult struct {
	Columns []string        `json:"columns"`
	Rows    [][]interface{} `json:"rows"`
	Stats   *QueryStats     `json:"stats,omitempty"`
}

// RowsAsMap converts the array-based rows to map-based rows using column names as keys.
func (qr *QueryResult) RowsAsMap() []map[string]interface{} {
	result := make([]map[string]interface{}, len(qr.Rows))
	for i, row := range qr.Rows {
		rowMap := make(map[string]interface{})
		for j, col := range qr.Columns {
			if j < len(row) {
				rowMap[col] = row[j]
			}
		}
		result[i] = rowMap
	}
	return result
}

// QueryStats contains execution statistics for a query.
type QueryStats struct {
	NodesCreated         int     `json:"nodes_created"`
	NodesDeleted         int     `json:"nodes_deleted"`
	RelationshipsCreated int     `json:"relationships_created"`
	RelationshipsDeleted int     `json:"relationships_deleted"`
	PropertiesSet        int     `json:"properties_set"`
	ExecutionTimeMs      float64 `json:"execution_time_ms"`
}

// Node represents a graph node.
type Node struct {
	ID         string                 `json:"id"`
	Labels     []string               `json:"labels"`
	Properties map[string]interface{} `json:"properties"`
}

// Relationship represents a graph relationship.
type Relationship struct {
	ID         string                 `json:"id"`
	Type       string                 `json:"type"`
	StartNode  string                 `json:"start_node"`
	EndNode    string                 `json:"end_node"`
	Properties map[string]interface{} `json:"properties"`
}

// Error represents a Nexus API error.
type Error struct {
	StatusCode int
	Message    string
}

func (e *Error) Error() string {
	return fmt.Sprintf("nexus: HTTP %d: %s", e.StatusCode, e.Message)
}

// doRequest performs an HTTP request with authentication.
func (c *Client) doRequest(ctx context.Context, method, path string, body interface{}) (*http.Response, error) {
	var reqBody io.Reader
	if body != nil {
		jsonData, err := json.Marshal(body)
		if err != nil {
			return nil, fmt.Errorf("failed to marshal request body: %w", err)
		}
		reqBody = bytes.NewReader(jsonData)
	}

	reqURL, err := url.JoinPath(c.baseURL, path)
	if err != nil {
		return nil, fmt.Errorf("failed to build URL: %w", err)
	}

	req, err := http.NewRequestWithContext(ctx, method, reqURL, reqBody)
	if err != nil {
		return nil, fmt.Errorf("failed to create request: %w", err)
	}

	req.Header.Set("Content-Type", "application/json")

	// Add authentication
	if c.apiKey != "" {
		req.Header.Set("X-API-Key", c.apiKey)
	} else if c.token != "" {
		req.Header.Set("Authorization", "Bearer "+c.token)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("request failed: %w", err)
	}

	if resp.StatusCode >= 400 {
		defer resp.Body.Close()
		bodyBytes, _ := io.ReadAll(resp.Body)
		return nil, &Error{
			StatusCode: resp.StatusCode,
			Message:    string(bodyBytes),
		}
	}

	return resp, nil
}

// ExecuteCypher executes a Cypher query and returns the results.
func (c *Client) ExecuteCypher(ctx context.Context, query string, params map[string]interface{}) (*QueryResult, error) {
	reqBody := map[string]interface{}{
		"query": query,
	}
	if params != nil {
		reqBody["parameters"] = params
	}

	resp, err := c.doRequest(ctx, http.MethodPost, "/cypher", reqBody)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var result QueryResult
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return &result, nil
}

// CreateNode creates a new node with the given labels and properties.
func (c *Client) CreateNode(ctx context.Context, labels []string, properties map[string]interface{}) (*Node, error) {
	reqBody := map[string]interface{}{
		"labels":     labels,
		"properties": properties,
	}

	resp, err := c.doRequest(ctx, http.MethodPost, "/nodes", reqBody)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var node Node
	if err := json.NewDecoder(resp.Body).Decode(&node); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return &node, nil
}

// GetNode retrieves a node by its ID.
func (c *Client) GetNode(ctx context.Context, id string) (*Node, error) {
	path := fmt.Sprintf("/nodes/%s", url.PathEscape(id))
	resp, err := c.doRequest(ctx, http.MethodGet, path, nil)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var node Node
	if err := json.NewDecoder(resp.Body).Decode(&node); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return &node, nil
}

// UpdateNode updates a node's properties.
func (c *Client) UpdateNode(ctx context.Context, id string, properties map[string]interface{}) (*Node, error) {
	reqBody := map[string]interface{}{
		"properties": properties,
	}

	path := fmt.Sprintf("/nodes/%s", url.PathEscape(id))
	resp, err := c.doRequest(ctx, http.MethodPut, path, reqBody)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var node Node
	if err := json.NewDecoder(resp.Body).Decode(&node); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return &node, nil
}

// DeleteNode deletes a node by its ID.
func (c *Client) DeleteNode(ctx context.Context, id string) error {
	path := fmt.Sprintf("/nodes/%s", url.PathEscape(id))
	resp, err := c.doRequest(ctx, http.MethodDelete, path, nil)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	return nil
}

// CreateRelationship creates a new relationship between two nodes.
func (c *Client) CreateRelationship(ctx context.Context, startNode, endNode, relType string, properties map[string]interface{}) (*Relationship, error) {
	reqBody := map[string]interface{}{
		"start_node": startNode,
		"end_node":   endNode,
		"type":       relType,
		"properties": properties,
	}

	resp, err := c.doRequest(ctx, http.MethodPost, "/relationships", reqBody)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var rel Relationship
	if err := json.NewDecoder(resp.Body).Decode(&rel); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return &rel, nil
}

// GetRelationship retrieves a relationship by its ID.
func (c *Client) GetRelationship(ctx context.Context, id string) (*Relationship, error) {
	path := fmt.Sprintf("/relationships/%s", url.PathEscape(id))
	resp, err := c.doRequest(ctx, http.MethodGet, path, nil)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var rel Relationship
	if err := json.NewDecoder(resp.Body).Decode(&rel); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return &rel, nil
}

// DeleteRelationship deletes a relationship by its ID.
func (c *Client) DeleteRelationship(ctx context.Context, id string) error {
	path := fmt.Sprintf("/relationships/%s", url.PathEscape(id))
	resp, err := c.doRequest(ctx, http.MethodDelete, path, nil)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	return nil
}

// Ping checks if the server is reachable.
func (c *Client) Ping(ctx context.Context) error {
	resp, err := c.doRequest(ctx, http.MethodGet, "/health", nil)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	return nil
}

// BatchCreateNodes creates multiple nodes in a single request.
func (c *Client) BatchCreateNodes(ctx context.Context, nodes []struct {
	Labels     []string
	Properties map[string]interface{}
}) ([]Node, error) {
	reqBody := map[string]interface{}{
		"nodes": nodes,
	}

	resp, err := c.doRequest(ctx, http.MethodPost, "/batch/nodes", reqBody)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var result []Node
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return result, nil
}

// BatchCreateRelationships creates multiple relationships in a single request.
func (c *Client) BatchCreateRelationships(ctx context.Context, relationships []struct {
	StartNode  string
	EndNode    string
	Type       string
	Properties map[string]interface{}
}) ([]Relationship, error) {
	reqBody := map[string]interface{}{
		"relationships": relationships,
	}

	resp, err := c.doRequest(ctx, http.MethodPost, "/batch/relationships", reqBody)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var result []Relationship
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return result, nil
}

// ListLabels retrieves all node labels in the database.
func (c *Client) ListLabels(ctx context.Context) ([]string, error) {
	resp, err := c.doRequest(ctx, http.MethodGet, "/schema/labels", nil)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var result struct {
		Labels []string `json:"labels"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return result.Labels, nil
}

// ListRelationshipTypes retrieves all relationship types in the database.
func (c *Client) ListRelationshipTypes(ctx context.Context) ([]string, error) {
	resp, err := c.doRequest(ctx, http.MethodGet, "/schema/relationship-types", nil)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var result struct {
		Types []string `json:"types"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return result.Types, nil
}

// Index represents a database index.
type Index struct {
	Name       string   `json:"name"`
	Label      string   `json:"label"`
	Properties []string `json:"properties"`
	Type       string   `json:"type"`
}

// CreateIndex creates a new index on node properties.
func (c *Client) CreateIndex(ctx context.Context, name, label string, properties []string) error {
	reqBody := map[string]interface{}{
		"name":       name,
		"label":      label,
		"properties": properties,
	}

	resp, err := c.doRequest(ctx, http.MethodPost, "/schema/indexes", reqBody)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	return nil
}

// ListIndexes retrieves all indexes in the database.
func (c *Client) ListIndexes(ctx context.Context) ([]Index, error) {
	resp, err := c.doRequest(ctx, http.MethodGet, "/schema/indexes", nil)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var result struct {
		Indexes []Index `json:"indexes"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return result.Indexes, nil
}

// DeleteIndex deletes an index by name.
func (c *Client) DeleteIndex(ctx context.Context, name string) error {
	path := fmt.Sprintf("/schema/indexes/%s", url.PathEscape(name))
	resp, err := c.doRequest(ctx, http.MethodDelete, path, nil)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	return nil
}

// Transaction represents a database transaction.
type Transaction struct {
	client *Client
	id     string
}

// BeginTransaction starts a new transaction.
func (c *Client) BeginTransaction(ctx context.Context) (*Transaction, error) {
	resp, err := c.doRequest(ctx, http.MethodPost, "/transaction/begin", nil)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var result struct {
		TransactionID string `json:"transaction_id"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return &Transaction{
		client: c,
		id:     result.TransactionID,
	}, nil
}

// ExecuteCypher executes a Cypher query within the transaction.
func (tx *Transaction) ExecuteCypher(ctx context.Context, query string, params map[string]interface{}) (*QueryResult, error) {
	reqBody := map[string]interface{}{
		"query":          query,
		"transaction_id": tx.id,
	}
	if params != nil {
		reqBody["parameters"] = params
	}

	resp, err := tx.client.doRequest(ctx, http.MethodPost, "/transaction/execute", reqBody)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var result QueryResult
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return nil, fmt.Errorf("failed to decode response: %w", err)
	}

	return &result, nil
}

// Commit commits the transaction.
func (tx *Transaction) Commit(ctx context.Context) error {
	reqBody := map[string]interface{}{
		"transaction_id": tx.id,
	}

	resp, err := tx.client.doRequest(ctx, http.MethodPost, "/transaction/commit", reqBody)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	return nil
}

// Rollback rolls back the transaction.
func (tx *Transaction) Rollback(ctx context.Context) error {
	reqBody := map[string]interface{}{
		"transaction_id": tx.id,
	}

	resp, err := tx.client.doRequest(ctx, http.MethodPost, "/transaction/rollback", reqBody)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	return nil
}

// decodeResponse is a helper function to decode HTTP responses.
func decodeResponse(resp *http.Response, v interface{}) error {
	if err := json.NewDecoder(resp.Body).Decode(v); err != nil {
		return fmt.Errorf("failed to decode response: %w", err)
	}
	return nil
}
