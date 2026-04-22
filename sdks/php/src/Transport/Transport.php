<?php

declare(strict_types=1);

namespace Nexus\SDK\Transport;

/**
 * Credentials carried by a transport. Both paths may be set; apiKey
 * takes precedence.
 */
final class Credentials
{
    public function __construct(
        public readonly ?string $apiKey = null,
        public readonly ?string $username = null,
        public readonly ?string $password = null,
    ) {
    }

    public function hasAny(): bool
    {
        return ($this->apiKey !== null && $this->apiKey !== '')
            || ($this->username !== null && $this->username !== ''
                && $this->password !== null && $this->password !== '');
    }
}

/**
 * Generic transport interface — one method per request/response pair.
 */
interface Transport
{
    /**
     * Send a single request and wait for the matching response.
     *
     * @param NexusValue[] $args
     */
    public function execute(string $command, array $args): NexusValue;

    public function describe(): string;

    public function isRpc(): bool;

    public function close(): void;
}

/**
 * Structured HTTP error surfaced by HttpTransport on non-2xx
 * responses. Callers can type-check with `catch (HttpRpcException)`
 * to recover the status code without parsing error strings.
 */
final class HttpRpcException extends \RuntimeException
{
    public function __construct(
        public readonly int $statusCode,
        public readonly string $body,
    ) {
        parent::__construct(sprintf('HTTP %d: %s', $statusCode, $body));
    }
}
