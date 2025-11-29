<?php

declare(strict_types=1);

namespace Nexus\SDK;

use Exception;

/**
 * Base exception for Nexus SDK.
 */
class NexusException extends Exception
{
}

/**
 * Exception thrown when HTTP request fails.
 */
class NexusApiException extends NexusException
{
    public function __construct(
        public readonly int $statusCode,
        public readonly string $responseBody,
        string $message = ''
    ) {
        $msg = $message ?: "Nexus API error: HTTP {$statusCode}: {$responseBody}";
        parent::__construct($msg);
    }
}

/**
 * Exception thrown for transaction errors.
 */
class NexusTransactionException extends NexusException
{
}
