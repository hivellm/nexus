<?php

declare(strict_types=1);

namespace Nexus\SDK;

use GuzzleHttp\Exception\ConnectException;
use GuzzleHttp\Exception\RequestException;

/**
 * Configuration for retry behavior.
 */
class RetryConfig
{
    /**
     * @param int[] $retryableStatusCodes HTTP status codes that should trigger a retry
     */
    public function __construct(
        /** Maximum number of retry attempts (default: 3) */
        public int $maxRetries = 3,
        /** Initial backoff duration in milliseconds (default: 100) */
        public int $initialBackoffMs = 100,
        /** Maximum backoff duration in milliseconds (default: 10000) */
        public int $maxBackoffMs = 10000,
        /** Multiplier for exponential backoff (default: 2.0) */
        public float $backoffMultiplier = 2.0,
        /** Whether to add jitter to backoff (default: true) */
        public bool $jitter = true,
        /** HTTP status codes that should trigger a retry */
        public array $retryableStatusCodes = [
            408, // Request Timeout
            429, // Too Many Requests
            500, // Internal Server Error
            502, // Bad Gateway
            503, // Service Unavailable
            504, // Gateway Timeout
        ]
    ) {
    }

    /**
     * Create a default retry configuration.
     */
    public static function default(): self
    {
        return new self();
    }

    /**
     * Check if an exception is retryable.
     */
    public function isRetryable(\Throwable $e): bool
    {
        // Connection errors are retryable
        if ($e instanceof ConnectException) {
            return true;
        }

        // Check HTTP status codes
        if ($e instanceof NexusApiException) {
            return in_array($e->statusCode, $this->retryableStatusCodes, true);
        }

        if ($e instanceof RequestException && $e->hasResponse()) {
            $statusCode = $e->getResponse()?->getStatusCode();
            return $statusCode !== null && in_array($statusCode, $this->retryableStatusCodes, true);
        }

        return false;
    }

    /**
     * Calculate backoff duration for a given attempt.
     */
    public function calculateBackoffMs(int $attempt): int
    {
        $backoff = $this->initialBackoffMs * pow($this->backoffMultiplier, $attempt);

        if ($this->jitter) {
            // Add ±25% jitter
            $jitterRange = $backoff * 0.25;
            $backoff = $backoff - $jitterRange + (mt_rand() / mt_getrandmax() * $jitterRange * 2);
        }

        return min((int) $backoff, $this->maxBackoffMs);
    }
}

/**
 * A wrapper around NexusClient that adds automatic retry functionality.
 */
class RetryableClient
{
    private NexusClient $client;
    private RetryConfig $retryConfig;

    public function __construct(Config $config, ?RetryConfig $retryConfig = null)
    {
        $this->client = new NexusClient($config);
        $this->retryConfig = $retryConfig ?? RetryConfig::default();
    }

    /**
     * Create from an existing client.
     */
    public static function fromClient(NexusClient $client, ?RetryConfig $retryConfig = null): self
    {
        $instance = new self(new Config(), $retryConfig);
        $instance->client = $client;
        return $instance;
    }

    /**
     * Set bearer token for authentication.
     */
    public function setToken(string $token): void
    {
        $this->client->setToken($token);
    }

    /**
     * Execute an operation with automatic retry.
     *
     * @template T
     * @param callable(): T $operation
     * @return T
     * @throws \Throwable
     */
    private function executeWithRetry(callable $operation): mixed
    {
        $lastException = null;

        for ($attempt = 0; $attempt <= $this->retryConfig->maxRetries; $attempt++) {
            try {
                return $operation();
            } catch (\Throwable $e) {
                $lastException = $e;

                if (!$this->retryConfig->isRetryable($e)) {
                    throw $e;
                }

                if ($attempt < $this->retryConfig->maxRetries) {
                    $backoffMs = $this->retryConfig->calculateBackoffMs($attempt);
                    usleep($backoffMs * 1000); // Convert to microseconds
                }
            }
        }

        throw $lastException ?? new NexusException('Retry failed without exception');
    }

    /**
     * Check if server is reachable with automatic retry.
     *
     * @throws NexusApiException
     */
    public function ping(): void
    {
        $this->executeWithRetry(fn() => $this->client->ping());
    }

    /**
     * Execute a Cypher query with automatic retry.
     *
     * @param array<string, mixed>|null $parameters
     * @throws NexusApiException
     */
    public function executeCypher(string $query, ?array $parameters = null): QueryResult
    {
        return $this->executeWithRetry(fn() => $this->client->executeCypher($query, $parameters));
    }

    /**
     * Create a new node with automatic retry.
     *
     * @param string[] $labels
     * @param array<string, mixed> $properties
     * @throws NexusApiException
     */
    public function createNode(array $labels, array $properties): Node
    {
        return $this->executeWithRetry(fn() => $this->client->createNode($labels, $properties));
    }

    /**
     * Get a node by ID with automatic retry.
     *
     * @throws NexusApiException
     */
    public function getNode(string $id): Node
    {
        return $this->executeWithRetry(fn() => $this->client->getNode($id));
    }

    /**
     * Update a node's properties with automatic retry.
     *
     * @param array<string, mixed> $properties
     * @throws NexusApiException
     */
    public function updateNode(string $id, array $properties): Node
    {
        return $this->executeWithRetry(fn() => $this->client->updateNode($id, $properties));
    }

    /**
     * Delete a node with automatic retry.
     *
     * @throws NexusApiException
     */
    public function deleteNode(string $id): void
    {
        $this->executeWithRetry(fn() => $this->client->deleteNode($id));
    }

    /**
     * Create a relationship with automatic retry.
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
        return $this->executeWithRetry(
            fn() => $this->client->createRelationship($startNode, $endNode, $type, $properties)
        );
    }

    /**
     * List all labels with automatic retry.
     *
     * @return string[]
     * @throws NexusApiException
     */
    public function listLabels(): array
    {
        return $this->executeWithRetry(fn() => $this->client->listLabels());
    }

    /**
     * List all relationship types with automatic retry.
     *
     * @return string[]
     * @throws NexusApiException
     */
    public function listRelationshipTypes(): array
    {
        return $this->executeWithRetry(fn() => $this->client->listRelationshipTypes());
    }

    /**
     * List all indexes with automatic retry.
     *
     * @return Index[]
     * @throws NexusApiException
     */
    public function listIndexes(): array
    {
        return $this->executeWithRetry(fn() => $this->client->listIndexes());
    }

    /**
     * Get the underlying client.
     */
    public function getClient(): NexusClient
    {
        return $this->client;
    }
}
