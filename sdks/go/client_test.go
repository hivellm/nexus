package nexus

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewClient(t *testing.T) {
	config := Config{
		BaseURL: "http://localhost:15474",
		APIKey:  "test-key",
		Timeout: 10 * time.Second,
	}

	client := NewClient(config)

	assert.NotNil(t, client)
	assert.Equal(t, config.BaseURL, client.baseURL)
	assert.Equal(t, config.APIKey, client.apiKey)
	assert.Equal(t, config.Timeout, client.httpClient.Timeout)
}

func TestNewClientDefaultTimeout(t *testing.T) {
	config := Config{
		BaseURL: "http://localhost:15474",
	}

	client := NewClient(config)

	assert.Equal(t, 30*time.Second, client.httpClient.Timeout)
}

func TestExecuteCypher(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/cypher", r.URL.Path)
		assert.Equal(t, "POST", r.Method)
		assert.Equal(t, "application/json", r.Header.Get("Content-Type"))

		var req map[string]interface{}
		err := json.NewDecoder(r.Body).Decode(&req)
		require.NoError(t, err)

		assert.Equal(t, "MATCH (n) RETURN n", req["query"])

		response := QueryResult{
			Columns: []string{"n"},
			Rows: [][]interface{}{
				{map[string]interface{}{"id": "1", "name": "Test"}},
			},
			Stats: &QueryStats{
				NodesCreated:    0,
				ExecutionTimeMs: 1.5,
			},
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	result, err := client.ExecuteCypher(ctx, "MATCH (n) RETURN n", nil)

	require.NoError(t, err)
	assert.Equal(t, []string{"n"}, result.Columns)
	assert.Len(t, result.Rows, 1)
	assert.NotNil(t, result.Stats)
	assert.Equal(t, 1.5, result.Stats.ExecutionTimeMs)
}

func TestExecuteCypherWithParams(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req map[string]interface{}
		err := json.NewDecoder(r.Body).Decode(&req)
		require.NoError(t, err)

		params, ok := req["parameters"].(map[string]interface{})
		assert.True(t, ok)
		assert.Equal(t, "John", params["name"])

		response := QueryResult{
			Columns: []string{"n"},
			Rows:    [][]interface{}{},
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	params := map[string]interface{}{
		"name": "John",
	}

	_, err := client.ExecuteCypher(ctx, "MATCH (n {name: $name}) RETURN n", params)
	require.NoError(t, err)
}

func TestCreateNode(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/nodes", r.URL.Path)
		assert.Equal(t, "POST", r.Method)

		var req map[string]interface{}
		err := json.NewDecoder(r.Body).Decode(&req)
		require.NoError(t, err)

		labels := req["labels"].([]interface{})
		assert.Contains(t, labels, "Person")

		response := Node{
			ID:     "1",
			Labels: []string{"Person"},
			Properties: map[string]interface{}{
				"name": "John",
			},
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	node, err := client.CreateNode(ctx, []string{"Person"}, map[string]interface{}{
		"name": "John",
	})

	require.NoError(t, err)
	assert.Equal(t, "1", node.ID)
	assert.Contains(t, node.Labels, "Person")
	assert.Equal(t, "John", node.Properties["name"])
}

func TestGetNode(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/nodes/1", r.URL.Path)
		assert.Equal(t, "GET", r.Method)

		response := Node{
			ID:     "1",
			Labels: []string{"Person"},
			Properties: map[string]interface{}{
				"name": "John",
			},
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	node, err := client.GetNode(ctx, "1")

	require.NoError(t, err)
	assert.Equal(t, "1", node.ID)
	assert.Contains(t, node.Labels, "Person")
}

func TestUpdateNode(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/nodes/1", r.URL.Path)
		assert.Equal(t, "PUT", r.Method)

		var req map[string]interface{}
		err := json.NewDecoder(r.Body).Decode(&req)
		require.NoError(t, err)

		props := req["properties"].(map[string]interface{})
		assert.Equal(t, "Jane", props["name"])

		response := Node{
			ID:     "1",
			Labels: []string{"Person"},
			Properties: map[string]interface{}{
				"name": "Jane",
			},
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	node, err := client.UpdateNode(ctx, "1", map[string]interface{}{
		"name": "Jane",
	})

	require.NoError(t, err)
	assert.Equal(t, "Jane", node.Properties["name"])
}

func TestDeleteNode(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/nodes/1", r.URL.Path)
		assert.Equal(t, "DELETE", r.Method)

		w.WriteHeader(http.StatusNoContent)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	err := client.DeleteNode(ctx, "1")

	require.NoError(t, err)
}

func TestCreateRelationship(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/relationships", r.URL.Path)
		assert.Equal(t, "POST", r.Method)

		var req map[string]interface{}
		err := json.NewDecoder(r.Body).Decode(&req)
		require.NoError(t, err)

		assert.Equal(t, "1", req["start_node"])
		assert.Equal(t, "2", req["end_node"])
		assert.Equal(t, "KNOWS", req["type"])

		response := Relationship{
			ID:        "r1",
			Type:      "KNOWS",
			StartNode: "1",
			EndNode:   "2",
			Properties: map[string]interface{}{
				"since": "2020",
			},
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	rel, err := client.CreateRelationship(ctx, "1", "2", "KNOWS", map[string]interface{}{
		"since": "2020",
	})

	require.NoError(t, err)
	assert.Equal(t, "r1", rel.ID)
	assert.Equal(t, "KNOWS", rel.Type)
	assert.Equal(t, "1", rel.StartNode)
	assert.Equal(t, "2", rel.EndNode)
}

func TestGetRelationship(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/relationships/r1", r.URL.Path)
		assert.Equal(t, "GET", r.Method)

		response := Relationship{
			ID:        "r1",
			Type:      "KNOWS",
			StartNode: "1",
			EndNode:   "2",
			Properties: map[string]interface{}{},
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	rel, err := client.GetRelationship(ctx, "r1")

	require.NoError(t, err)
	assert.Equal(t, "r1", rel.ID)
	assert.Equal(t, "KNOWS", rel.Type)
}

func TestDeleteRelationship(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/relationships/r1", r.URL.Path)
		assert.Equal(t, "DELETE", r.Method)

		w.WriteHeader(http.StatusNoContent)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	err := client.DeleteRelationship(ctx, "r1")

	require.NoError(t, err)
}

func TestPing(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/health", r.URL.Path)
		assert.Equal(t, "GET", r.Method)

		w.WriteHeader(http.StatusOK)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	err := client.Ping(ctx)

	require.NoError(t, err)
}

func TestAuthentication(t *testing.T) {
	t.Run("API Key", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			assert.Equal(t, "test-api-key", r.Header.Get("X-API-Key"))
			w.WriteHeader(http.StatusOK)
		}))
		defer server.Close()

		client := NewClient(Config{
			BaseURL: server.URL,
			APIKey:  "test-api-key",
		})

		err := client.Ping(context.Background())
		require.NoError(t, err)
	})

	t.Run("Bearer Token", func(t *testing.T) {
		server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			assert.Equal(t, "Bearer test-token", r.Header.Get("Authorization"))
			w.WriteHeader(http.StatusOK)
		}))
		defer server.Close()

		client := NewClient(Config{BaseURL: server.URL})
		client.token = "test-token"

		err := client.Ping(context.Background())
		require.NoError(t, err)
	})
}

func TestErrorHandling(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusBadRequest)
		w.Write([]byte("Invalid query syntax"))
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	_, err := client.ExecuteCypher(ctx, "INVALID QUERY", nil)

	require.Error(t, err)
	nexusErr, ok := err.(*Error)
	require.True(t, ok)
	assert.Equal(t, http.StatusBadRequest, nexusErr.StatusCode)
	assert.Contains(t, nexusErr.Message, "Invalid query syntax")
}

func TestBatchCreateNodes(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/batch/nodes", r.URL.Path)
		assert.Equal(t, "POST", r.Method)

		var req map[string]interface{}
		err := json.NewDecoder(r.Body).Decode(&req)
		require.NoError(t, err)

		nodes := req["nodes"].([]interface{})
		assert.Len(t, nodes, 2)

		response := []Node{
			{ID: "1", Labels: []string{"Person"}, Properties: map[string]interface{}{"name": "John"}},
			{ID: "2", Labels: []string{"Person"}, Properties: map[string]interface{}{"name": "Jane"}},
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	nodes, err := client.BatchCreateNodes(ctx, []struct {
		Labels     []string
		Properties map[string]interface{}
	}{
		{Labels: []string{"Person"}, Properties: map[string]interface{}{"name": "John"}},
		{Labels: []string{"Person"}, Properties: map[string]interface{}{"name": "Jane"}},
	})

	require.NoError(t, err)
	assert.Len(t, nodes, 2)
	assert.Equal(t, "1", nodes[0].ID)
	assert.Equal(t, "2", nodes[1].ID)
}

func TestListLabels(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/schema/labels", r.URL.Path)
		assert.Equal(t, "GET", r.Method)

		response := map[string]interface{}{
			"labels": []string{"Person", "Company"},
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	labels, err := client.ListLabels(ctx)

	require.NoError(t, err)
	assert.Contains(t, labels, "Person")
	assert.Contains(t, labels, "Company")
}

func TestListRelationshipTypes(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/schema/relationship-types", r.URL.Path)
		assert.Equal(t, "GET", r.Method)

		response := map[string]interface{}{
			"types": []string{"KNOWS", "WORKS_AT"},
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	types, err := client.ListRelationshipTypes(ctx)

	require.NoError(t, err)
	assert.Contains(t, types, "KNOWS")
	assert.Contains(t, types, "WORKS_AT")
}

func TestCreateIndex(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/schema/indexes", r.URL.Path)
		assert.Equal(t, "POST", r.Method)

		var req map[string]interface{}
		err := json.NewDecoder(r.Body).Decode(&req)
		require.NoError(t, err)

		assert.Equal(t, "person_name_idx", req["name"])
		assert.Equal(t, "Person", req["label"])

		w.WriteHeader(http.StatusCreated)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	err := client.CreateIndex(ctx, "person_name_idx", "Person", []string{"name"})

	require.NoError(t, err)
}

func TestListIndexes(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		assert.Equal(t, "/schema/indexes", r.URL.Path)
		assert.Equal(t, "GET", r.Method)

		response := map[string]interface{}{
			"indexes": []Index{
				{Name: "person_name_idx", Label: "Person", Properties: []string{"name"}, Type: "btree"},
			},
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(response)
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	indexes, err := client.ListIndexes(ctx)

	require.NoError(t, err)
	assert.Len(t, indexes, 1)
	assert.Equal(t, "person_name_idx", indexes[0].Name)
}

func TestTransactionWorkflow(t *testing.T) {
	transactionID := "tx-123"

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/transaction/begin":
			assert.Equal(t, "POST", r.Method)
			response := map[string]interface{}{
				"transaction_id": transactionID,
			}
			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(response)

		case "/transaction/execute":
			assert.Equal(t, "POST", r.Method)
			var req map[string]interface{}
			err := json.NewDecoder(r.Body).Decode(&req)
			require.NoError(t, err)
			assert.Equal(t, transactionID, req["transaction_id"])

			response := QueryResult{
				Columns: []string{"n"},
				Rows:    [][]interface{}{},
			}
			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(response)

		case "/transaction/commit":
			assert.Equal(t, "POST", r.Method)
			var req map[string]interface{}
			err := json.NewDecoder(r.Body).Decode(&req)
			require.NoError(t, err)
			assert.Equal(t, transactionID, req["transaction_id"])
			w.WriteHeader(http.StatusOK)

		default:
			t.Fatalf("Unexpected path: %s", r.URL.Path)
		}
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	// Begin transaction
	tx, err := client.BeginTransaction(ctx)
	require.NoError(t, err)
	assert.Equal(t, transactionID, tx.id)

	// Execute query in transaction
	_, err = tx.ExecuteCypher(ctx, "CREATE (n:Person {name: 'John'})", nil)
	require.NoError(t, err)

	// Commit transaction
	err = tx.Commit(ctx)
	require.NoError(t, err)
}

func TestTransactionRollback(t *testing.T) {
	transactionID := "tx-456"

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/transaction/begin":
			response := map[string]interface{}{
				"transaction_id": transactionID,
			}
			w.Header().Set("Content-Type", "application/json")
			json.NewEncoder(w).Encode(response)

		case "/transaction/rollback":
			assert.Equal(t, "POST", r.Method)
			var req map[string]interface{}
			err := json.NewDecoder(r.Body).Decode(&req)
			require.NoError(t, err)
			assert.Equal(t, transactionID, req["transaction_id"])
			w.WriteHeader(http.StatusOK)

		default:
			t.Fatalf("Unexpected path: %s", r.URL.Path)
		}
	}))
	defer server.Close()

	client := NewClient(Config{BaseURL: server.URL})
	ctx := context.Background()

	// Begin transaction
	tx, err := client.BeginTransaction(ctx)
	require.NoError(t, err)

	// Rollback transaction
	err = tx.Rollback(ctx)
	require.NoError(t, err)
}
