<?php

declare(strict_types=1);

namespace Nexus\SDK;

use GuzzleHttp\Client;
use GuzzleHttp\Exception\GuzzleException;
use GuzzleHttp\RequestOptions;
use Nexus\SDK\Transport\CommandMap;
use Nexus\SDK\Transport\Credentials;
use Nexus\SDK\Transport\Endpoint;
use Nexus\SDK\Transport\HttpRpcException;
use Nexus\SDK\Transport\NexusValue;
use Nexus\SDK\Transport\Transport;
use Nexus\SDK\Transport\TransportFactory;
use Nexus\SDK\Transport\TransportMode;

/**
 * Client for interacting with Nexus graph database.
 *
 * Defaults to the native binary RPC transport on
 * nexus://127.0.0.1:15475. Callers can opt down to HTTP with
 * Config::$transport = TransportMode::Http or by passing an
 * http:// URL as Config::$baseUrl.
 */
class NexusClient
{
    private Client $httpClient;
    private ?string $token = null;
    private readonly Transport $transport;
    private readonly Endpoint $endpoint;
    private readonly TransportMode $mode;

    public function __construct(private readonly Config $config)
    {
        $built = TransportFactory::build(
            $config->baseUrl,
            new Credentials(
                apiKey: $config->apiKey,
                username: $config->username,
                password: $config->password,
            ),
            transportHint: $config->transport,
            rpcPort: $config->rpcPort,
            resp3Port: $config->resp3Port,
            timeoutS: $config->timeout,
        );
        $this->transport = $built['transport'];
        $this->endpoint = $built['endpoint'];
        $this->mode = $built['mode'];

        // REST-specific convenience methods keep a sibling httpClient
        // against the sibling HTTP port so CRUD helpers (CreateNode, …)
        // continue to work even when the primary transport is RPC.
        $this->httpClient = new Client([
            'base_uri' => $this->endpoint->asHttpUrl(),
            'timeout' => $config->timeout,
            'headers' => [
                'Accept' => 'application/json',
                'Content-Type' => 'application/json',
            ],
        ]);
    }

    /** Active transport mode after the precedence chain was resolved. */
    public function getTransportMode(): TransportMode
    {
        return $this->mode;
    }

    /** Human-readable endpoint + transport label. */
    public function endpointDescription(): string
    {
        return $this->transport->describe();
    }

    /** Release the persistent RPC socket (if any). */
    public function close(): void
    {
        $this->transport->close();
    }

    public function __destruct()
    {
        $this->close();
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
     * Execute a Cypher query via the active transport.
     *
     * @param array<string, mixed>|null $parameters
     * @throws NexusApiException
     */
    public function executeCypher(string $query, ?array $parameters = null): QueryResult
    {
        $args = [NexusValue::str($query)];
        if ($parameters !== null) {
            $args[] = CommandMap::jsonToNexus($parameters);
        }
        try {
            $resp = $this->transport->execute('CYPHER', $args);
        } catch (HttpRpcException $e) {
            throw new NexusApiException($e->body, $e->statusCode);
        } catch (\RuntimeException $e) {
            throw new NexusApiException($e->getMessage(), 0, $e);
        }
        $json = CommandMap::nexusToJson($resp);
        if (!is_array($json)) {
            throw new NexusApiException(sprintf(
                'CYPHER: expected object response, got %s',
                get_debug_type($json),
            ));
        }
        return QueryResult::fromArray($json);
    }

    /**
     * Create a new node.
     *
     * @param string[] $labels
     * @param array<string, mixed> $properties
     * @param string|null $externalId Optional prefixed external id (e.g. "str:my-key",
     *        "sha256:<hex>", "blake3:<hex>", "sha512:<hex>", "uuid:<canonical>",
     *        "bytes:<hex>"). Omitted when null.
     * @param string|null $conflictPolicy Optional conflict policy: "error" (default),
     *        "match", or "replace". Omitted when null.
     * @throws NexusApiException
     */
    public function createNode(
        array $labels,
        array $properties,
        ?string $externalId = null,
        ?string $conflictPolicy = null
    ): Node {
        $body = [
            'labels' => $labels,
            'properties' => $properties,
        ];
        if ($externalId !== null) {
            $body['external_id'] = $externalId;
        }
        if ($conflictPolicy !== null) {
            $body['conflict_policy'] = $conflictPolicy;
        }

        $response = $this->doRequest('POST', '/nodes', $body);
        return Node::fromArray($response);
    }

    /**
     * Create a new node with a caller-supplied external id.
     *
     * Convenience wrapper around createNode() for the common case where
     * an external id is required.
     *
     * @param string[] $labels
     * @param array<string, mixed> $properties
     * @param string $externalId Prefixed string form (e.g. "str:my-key",
     *        "sha256:<hex>", "blake3:<hex>", "sha512:<hex>", "uuid:<canonical>",
     *        "bytes:<hex>").
     * @param string|null $conflictPolicy "error" (default), "match", or "replace".
     * @throws NexusApiException
     */
    public function createNodeWithExternalId(
        array $labels,
        array $properties,
        string $externalId,
        ?string $conflictPolicy = null
    ): Node {
        return $this->createNode($labels, $properties, $externalId, $conflictPolicy);
    }

    /**
     * Resolve a node by its external id.
     *
     * Returns null for the `node` key in the response when no matching
     * node exists. The raw decoded response array is returned so callers
     * can inspect the `message` and `error` fields as well.
     *
     * @return array{node: array<string, mixed>|null, message: string, error: string|null}
     * @throws NexusApiException
     */
    public function getNodeByExternalId(string $externalId): array
    {
        $path = '/data/nodes/by-external-id?external_id=' . urlencode($externalId);
        /** @var array{node: array<string, mixed>|null, message: string, error: string|null} */
        return $this->doRequest('GET', $path);
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
     * Each entry is the JSON object `{"name": "Person", "id": 0}`
     * returned by the server. The `id` field is the catalog id
     * allocated by the engine, not a count. Renamed from a JSON
     * tuple `["Person", 0]` in nexus-server 1.15+ — see
     * https://github.com/hivellm/nexus/issues/2.
     *
     * @return array<int, array{name: string, id: int}>
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
     * Each entry is the JSON object `{"name": "...", "id": ...}`.
     * Server route is `/schema/rel_types` (this SDK previously used
     * the non-existent `/schema/relationship-types`).
     *
     * @return array<int, array{name: string, id: int}>
     * @throws NexusApiException
     */
    public function listRelationshipTypes(): array
    {
        $response = $this->doRequest('GET', '/schema/rel_types');
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
