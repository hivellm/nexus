<?php

declare(strict_types=1);

namespace Nexus\SDK;

/**
 * Represents a graph node.
 */
class Node
{
    public function __construct(
        public string $id = '',
        /** @var string[] */
        public array $labels = [],
        /** @var array<string, mixed> */
        public array $properties = []
    ) {
    }

    /**
     * Create from API response.
     *
     * @param array<string, mixed> $data
     */
    public static function fromArray(array $data): self
    {
        return new self(
            id: $data['id'] ?? '',
            labels: $data['labels'] ?? [],
            properties: $data['properties'] ?? []
        );
    }
}

/**
 * Response from POST /data/nodes — the server returns a node_id +
 * message + optional error envelope, not a full Node object. Phase 11
 * §2.1 — restored after the 2.0/2.1 ports incorrectly tried to
 * deserialise the response as a `Node`.
 */
class CreateNodeResponse
{
    public function __construct(
        public int $nodeId = 0,
        public string $message = '',
        public ?string $error = null
    ) {
    }

    /**
     * @param array<string, mixed> $data
     */
    public static function fromArray(array $data): self
    {
        return new self(
            nodeId: (int) ($data['node_id'] ?? 0),
            message: (string) ($data['message'] ?? ''),
            error: isset($data['error']) ? (string) $data['error'] : null
        );
    }
}

/**
 * Response from GET /data/nodes?id=<id> — the envelope wraps an
 * optional Node. `error` is set on lookup failure; `node` is null when
 * the id is absent.
 */
class GetNodeResponse
{
    public function __construct(
        public ?Node $node = null,
        public string $message = '',
        public ?string $error = null
    ) {
    }

    /**
     * @param array<string, mixed> $data
     */
    public static function fromArray(array $data): self
    {
        $rawNode = $data['node'] ?? null;
        return new self(
            node: is_array($rawNode) ? Node::fromArray($rawNode) : null,
            message: (string) ($data['message'] ?? ''),
            error: isset($data['error']) ? (string) $data['error'] : null
        );
    }
}

/**
 * Represents a graph relationship.
 */
class Relationship
{
    public function __construct(
        public string $id = '',
        public string $type = '',
        public string $startNode = '',
        public string $endNode = '',
        /** @var array<string, mixed> */
        public array $properties = []
    ) {
    }

    /**
     * Create from API response.
     *
     * @param array<string, mixed> $data
     */
    public static function fromArray(array $data): self
    {
        return new self(
            id: $data['id'] ?? '',
            type: $data['type'] ?? '',
            startNode: $data['start_node'] ?? '',
            endNode: $data['end_node'] ?? '',
            properties: $data['properties'] ?? []
        );
    }
}

/**
 * Query execution statistics.
 */
class QueryStats
{
    public function __construct(
        public int $nodesCreated = 0,
        public int $nodesDeleted = 0,
        public int $relationshipsCreated = 0,
        public int $relationshipsDeleted = 0,
        public int $propertiesSet = 0,
        public float $executionTimeMs = 0.0
    ) {
    }

    /**
     * Create from API response.
     *
     * @param array<string, mixed> $data
     */
    public static function fromArray(array $data): self
    {
        return new self(
            nodesCreated: $data['nodes_created'] ?? 0,
            nodesDeleted: $data['nodes_deleted'] ?? 0,
            relationshipsCreated: $data['relationships_created'] ?? 0,
            relationshipsDeleted: $data['relationships_deleted'] ?? 0,
            propertiesSet: $data['properties_set'] ?? 0,
            executionTimeMs: $data['execution_time_ms'] ?? 0.0
        );
    }
}

/**
 * Result of a Cypher query.
 */
class QueryResult
{
    /**
     * @param string[] $columns
     * @param array<int, array<string, mixed>> $rows
     */
    public function __construct(
        public array $columns = [],
        public array $rows = [],
        public ?QueryStats $stats = null,
        public ?string $error = null
    ) {
    }

    /**
     * Create from API response.
     *
     * @param array<string, mixed> $data
     */
    public static function fromArray(array $data): self
    {
        $stats = isset($data['stats']) ? QueryStats::fromArray($data['stats']) : null;

        return new self(
            columns: $data['columns'] ?? [],
            rows: $data['rows'] ?? [],
            stats: $stats,
            error: $data['error'] ?? null
        );
    }
}

/**
 * Database index.
 */
class Index
{
    /**
     * @param string[] $properties
     */
    public function __construct(
        public string $name = '',
        public string $label = '',
        public array $properties = [],
        public string $type = ''
    ) {
    }

    /**
     * Create from API response.
     *
     * @param array<string, mixed> $data
     */
    public static function fromArray(array $data): self
    {
        return new self(
            name: $data['name'] ?? '',
            label: $data['label'] ?? '',
            properties: $data['properties'] ?? [],
            type: $data['type'] ?? ''
        );
    }
}

/**
 * Configuration for Nexus client.
 *
 * Transport precedence: URL scheme in $baseUrl > NEXUS_SDK_TRANSPORT
 * env var > $transport field > default (binary RPC).
 */
class Config
{
    public function __construct(
        public string $baseUrl = 'nexus://127.0.0.1:15475',
        public ?string $apiKey = null,
        public ?string $username = null,
        public ?string $password = null,
        public int $timeout = 30,
        public ?\Nexus\SDK\Transport\TransportMode $transport = null,
        public ?int $rpcPort = null,
        public ?int $resp3Port = null,
    ) {
    }
}
