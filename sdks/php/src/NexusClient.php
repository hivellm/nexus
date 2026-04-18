<?php

declare(strict_types=1);

namespace Nexus\SDK;

use GuzzleHttp\Client;
use GuzzleHttp\Exception\GuzzleException;
use GuzzleHttp\RequestOptions;

/**
 * Client for interacting with Nexus graph database.
 */
class NexusClient
{
    private Client $httpClient;
    private ?string $token = null;

    public function __construct(private readonly Config $config)
    {
        $this->httpClient = new Client([
            'base_uri' => rtrim($config->baseUrl, '/'),
            'timeout' => $config->timeout,
            'headers' => [
                'Accept' => 'application/json',
                'Content-Type' => 'application/json',
            ],
        ]);
    }

    /**
     * Set bearer token for authentication.
     */
    public function setToken(string $token): void
    {
        $this->token = $token;
    }

    /**
     * Check if server is reachable.
     *
     * @throws NexusApiException
     */
    public function ping(): void
    {
        $this->doRequest('GET', '/health');
    }

    /**
     * Execute a Cypher query.
     *
     * @param array<string, mixed>|null $parameters
     * @throws NexusApiException
     */
    public function executeCypher(string $query, ?array $parameters = null): QueryResult
    {
        $body = ['query' => $query];
        if ($parameters !== null) {
            $body['parameters'] = $parameters;
        }

        $response = $this->doRequest('POST', '/cypher', $body);
        return QueryResult::fromArray($response);
    }

    /**
     * Create a new node.
     *
     * @param string[] $labels
     * @param array<string, mixed> $properties
     * @throws NexusApiException
     */
    public function createNode(array $labels, array $properties): Node
    {
        $body = [
            'labels' => $labels,
            'properties' => $properties,
        ];

        $response = $this->doRequest('POST', '/nodes', $body);
        return Node::fromArray($response);
    }

    /**
     * Get a node by ID.
     *
     * @throws NexusApiException
     */
    public function getNode(string $id): Node
    {
        $response = $this->doRequest('GET', '/nodes/' . urlencode($id));
        return Node::fromArray($response);
    }

    /**
     * Update a node's properties.
     *
     * @param array<string, mixed> $properties
     * @throws NexusApiException
     */
    public function updateNode(string $id, array $properties): Node
    {
        $body = ['properties' => $properties];
        $response = $this->doRequest('PUT', '/nodes/' . urlencode($id), $body);
        return Node::fromArray($response);
    }

    /**
     * Delete a node.
     *
     * @throws NexusApiException
     */
    public function deleteNode(string $id): void
    {
        $this->doRequest('DELETE', '/nodes/' . urlencode($id));
    }

    /**
     * Batch create nodes.
     *
     * @param array<int, array{labels: string[], properties: array<string, mixed>}> $nodes
     * @return Node[]
     * @throws NexusApiException
     */
    public function batchCreateNodes(array $nodes): array
    {
        $body = ['nodes' => $nodes];
        $response = $this->doRequest('POST', '/batch/nodes', $body);

        return array_map(fn($data) => Node::fromArray($data), $response);
    }

    /**
     * Create a relationship.
     *
     * @param array<string, mixed> $properties
     * @throws NexusApiException
     */
    public function createRelationship(
        string $startNode,
        string $endNode,
        string $type,
        array $properties
    ): Relationship {
        $body = [
            'start_node' => $startNode,
            'end_node' => $endNode,
            'type' => $type,
            'properties' => $properties,
        ];

        $response = $this->doRequest('POST', '/relationships', $body);
        return Relationship::fromArray($response);
    }

    /**
     * Get a relationship by ID.
     *
     * @throws NexusApiException
     */
    public function getRelationship(string $id): Relationship
    {
        $response = $this->doRequest('GET', '/relationships/' . urlencode($id));
        return Relationship::fromArray($response);
    }

    /**
     * Delete a relationship.
     *
     * @throws NexusApiException
     */
    public function deleteRelationship(string $id): void
    {
        $this->doRequest('DELETE', '/relationships/' . urlencode($id));
    }

    /**
     * Batch create relationships.
     *
     * @param array<int, array{start_node: string, end_node: string, type: string, properties: array<string, mixed>}> $relationships
     * @return Relationship[]
     * @throws NexusApiException
     */
    public function batchCreateRelationships(array $relationships): array
    {
        $body = ['relationships' => $relationships];
        $response = $this->doRequest('POST', '/batch/relationships', $body);

        return array_map(fn($data) => Relationship::fromArray($data), $response);
    }

    /**
     * List all labels.
     *
     * @return string[]
     * @throws NexusApiException
     */
    public function listLabels(): array
    {
        $response = $this->doRequest('GET', '/schema/labels');
        return $response['labels'] ?? [];
    }

    /**
     * List all relationship types.
     *
     * @return string[]
     * @throws NexusApiException
     */
    public function listRelationshipTypes(): array
    {
        $response = $this->doRequest('GET', '/schema/relationship-types');
        return $response['types'] ?? [];
    }

    /**
     * Create an index.
     *
     * @param string[] $properties
     * @throws NexusApiException
     */
    public function createIndex(string $name, string $label, array $properties): void
    {
        $body = [
            'name' => $name,
            'label' => $label,
            'properties' => $properties,
        ];

        $this->doRequest('POST', '/schema/indexes', $body);
    }

    /**
     * List all indexes.
     *
     * @return Index[]
     * @throws NexusApiException
     */
    public function listIndexes(): array
    {
        $response = $this->doRequest('GET', '/schema/indexes');
        $indexes = $response['indexes'] ?? [];

        return array_map(fn($data) => Index::fromArray($data), $indexes);
    }

    /**
     * Delete an index.
     *
     * @throws NexusApiException
     */
    public function deleteIndex(string $name): void
    {
        $this->doRequest('DELETE', '/schema/indexes/' . urlencode($name));
    }

    /**
     * Begin a transaction.
     *
     * @throws NexusApiException|NexusTransactionException
     */
    public function beginTransaction(): Transaction
    {
        $response = $this->doRequest('POST', '/transaction/begin');
        $transactionId = $response['transaction_id'] ?? null;

        if (!$transactionId) {
            throw new NexusTransactionException('Failed to get transaction ID');
        }

        return new Transaction($this, $transactionId);
    }

    /**
     * Execute a query in a transaction.
     *
     * @param array<string, mixed>|null $parameters
     * @return array<string, mixed>
     * @throws NexusApiException
     * @internal
     */
    public function executeInTransaction(
        string $transactionId,
        string $query,
        ?array $parameters = null
    ): array {
        $body = [
            'query' => $query,
            'transaction_id' => $transactionId,
        ];

        if ($parameters !== null) {
            $body['parameters'] = $parameters;
        }

        return $this->doRequest('POST', '/transaction/execute', $body);
    }

    /**
     * Commit a transaction.
     *
     * @throws NexusApiException
     * @internal
     */
    public function commitTransaction(string $transactionId): void
    {
        $body = ['transaction_id' => $transactionId];
        $this->doRequest('POST', '/transaction/commit', $body);
    }

    /**
     * Rollback a transaction.
     *
     * @throws NexusApiException
     * @internal
     */
    public function rollbackTransaction(string $transactionId): void
    {
        $body = ['transaction_id' => $transactionId];
        $this->doRequest('POST', '/transaction/rollback', $body);
    }

    /**
     * Perform an HTTP request.
     *
     * @param array<string, mixed>|null $body
     * @return array<string, mixed>
     * @throws NexusApiException
     */
    private function doRequest(string $method, string $path, ?array $body = null): array
    {
        $options = [];

        // Add authentication
        if ($this->config->apiKey) {
            $options[RequestOptions::HEADERS]['X-API-Key'] = $this->config->apiKey;
        } elseif ($this->token) {
            $options[RequestOptions::HEADERS]['Authorization'] = "Bearer {$this->token}";
        }

        // Add body
        if ($body !== null) {
            $options[RequestOptions::JSON] = $body;
        }

        try {
            $response = $this->httpClient->request($method, $path, $options);
            $content = (string) $response->getBody();

            if ($content === '') {
                return [];
            }

            $decoded = json_decode($content, true);
            if (!is_array($decoded)) {
                return [];
            }

            return $decoded;
        } catch (GuzzleException $e) {
            $statusCode = $e->getCode();
            $message = $e->getMessage();

            if (method_exists($e, 'getResponse') && $e->getResponse()) {
                $statusCode = $e->getResponse()->getStatusCode();
                $message = (string) $e->getResponse()->getBody();
            }

            throw new NexusApiException($statusCode, $message);
        }
    }
}

/**
 * Represents a database transaction.
 */
class Transaction
{
    private bool $completed = false;

    /**
     * @internal
     */
    public function __construct(
        private readonly NexusClient $client,
        private readonly string $transactionId
    ) {
    }

    /**
     * Execute a Cypher query within the transaction.
     *
     * @param array<string, mixed>|null $parameters
     * @throws NexusApiException|NexusTransactionException
     */
    public function executeCypher(string $query, ?array $parameters = null): QueryResult
    {
        if ($this->completed) {
            throw new NexusTransactionException('Transaction has already been completed');
        }

        $response = $this->client->executeInTransaction($this->transactionId, $query, $parameters);
        return QueryResult::fromArray($response);
    }

    /**
     * Commit the transaction.
     *
     * @throws NexusApiException|NexusTransactionException
     */
    public function commit(): void
    {
        if ($this->completed) {
            throw new NexusTransactionException('Transaction has already been completed');
        }

        $this->client->commitTransaction($this->transactionId);
        $this->completed = true;
    }

    /**
     * Rollback the transaction.
     *
     * @throws NexusApiException|NexusTransactionException
     */
    public function rollback(): void
    {
        if ($this->completed) {
            throw new NexusTransactionException('Transaction has already been completed');
        }

        $this->client->rollbackTransaction($this->transactionId);
        $this->completed = true;
    }

    /**
     * Auto-rollback if not committed.
     */
    public function __destruct()
    {
        if (!$this->completed) {
            try {
                $this->rollback();
            } catch (\Throwable $e) {
                // Ignore errors during auto-rollback
            }
        }
    }
}
