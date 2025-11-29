<?php

declare(strict_types=1);

namespace Nexus\SDK\Tests;

use PHPUnit\Framework\TestCase;
use Nexus\SDK\RetryConfig;
use Nexus\SDK\NexusApiException;
use GuzzleHttp\Exception\ConnectException;
use GuzzleHttp\Psr7\Request;

class RetryConfigTest extends TestCase
{
    public function testDefaultConfig(): void
    {
        $config = RetryConfig::default();

        $this->assertEquals(3, $config->maxRetries);
        $this->assertEquals(100, $config->initialBackoffMs);
        $this->assertEquals(10000, $config->maxBackoffMs);
        $this->assertEquals(2.0, $config->backoffMultiplier);
        $this->assertTrue($config->jitter);
    }

    public function testCustomConfig(): void
    {
        $config = new RetryConfig(
            maxRetries: 5,
            initialBackoffMs: 200,
            maxBackoffMs: 5000,
            backoffMultiplier: 1.5,
            jitter: false
        );

        $this->assertEquals(5, $config->maxRetries);
        $this->assertEquals(200, $config->initialBackoffMs);
        $this->assertEquals(5000, $config->maxBackoffMs);
        $this->assertEquals(1.5, $config->backoffMultiplier);
        $this->assertFalse($config->jitter);
    }

    public function testIsRetryableWithApiException(): void
    {
        $config = RetryConfig::default();

        // Should retry 503 Service Unavailable
        $exception = new NexusApiException(503, 'Service Unavailable');
        $this->assertTrue($config->isRetryable($exception));

        // Should not retry 404 Not Found
        $exception = new NexusApiException(404, 'Not Found');
        $this->assertFalse($config->isRetryable($exception));

        // Should retry 429 Too Many Requests
        $exception = new NexusApiException(429, 'Too Many Requests');
        $this->assertTrue($config->isRetryable($exception));

        // Should retry 500 Internal Server Error
        $exception = new NexusApiException(500, 'Internal Server Error');
        $this->assertTrue($config->isRetryable($exception));
    }

    public function testIsRetryableWithConnectException(): void
    {
        $config = RetryConfig::default();

        $request = new Request('GET', 'http://localhost:15474/health');
        $exception = new ConnectException('Connection refused', $request);

        $this->assertTrue($config->isRetryable($exception));
    }

    public function testIsRetryableWithGenericException(): void
    {
        $config = RetryConfig::default();

        $exception = new \RuntimeException('Some error');
        $this->assertFalse($config->isRetryable($exception));
    }

    public function testCalculateBackoffWithoutJitter(): void
    {
        $config = new RetryConfig(
            initialBackoffMs: 100,
            backoffMultiplier: 2.0,
            jitter: false
        );

        $this->assertEquals(100, $config->calculateBackoffMs(0));
        $this->assertEquals(200, $config->calculateBackoffMs(1));
        $this->assertEquals(400, $config->calculateBackoffMs(2));
        $this->assertEquals(800, $config->calculateBackoffMs(3));
    }

    public function testCalculateBackoffWithMaxLimit(): void
    {
        $config = new RetryConfig(
            initialBackoffMs: 1000,
            maxBackoffMs: 3000,
            backoffMultiplier: 2.0,
            jitter: false
        );

        $this->assertEquals(1000, $config->calculateBackoffMs(0));
        $this->assertEquals(2000, $config->calculateBackoffMs(1));
        $this->assertEquals(3000, $config->calculateBackoffMs(2)); // Capped at max
        $this->assertEquals(3000, $config->calculateBackoffMs(3)); // Still capped
    }

    public function testCalculateBackoffWithJitter(): void
    {
        $config = new RetryConfig(
            initialBackoffMs: 100,
            backoffMultiplier: 2.0,
            jitter: true
        );

        // With jitter, backoff should be within ±25% of base value
        $baseBackoff = 100;
        $minExpected = (int) ($baseBackoff * 0.75);
        $maxExpected = (int) ($baseBackoff * 1.25);

        // Run multiple times to verify jitter adds randomness
        $values = [];
        for ($i = 0; $i < 10; $i++) {
            $backoff = $config->calculateBackoffMs(0);
            $values[] = $backoff;
            $this->assertGreaterThanOrEqual($minExpected, $backoff);
            $this->assertLessThanOrEqual($maxExpected, $backoff);
        }

        // Verify not all values are the same (jitter is working)
        $uniqueValues = array_unique($values);
        // Allow some possibility of collision but expect at least 2 different values
        $this->assertGreaterThan(1, count($uniqueValues));
    }

    public function testCustomRetryableStatusCodes(): void
    {
        $config = new RetryConfig(
            retryableStatusCodes: [500, 502, 503]
        );

        // Should retry 500
        $this->assertTrue($config->isRetryable(new NexusApiException(500, 'Error')));

        // Should NOT retry 429 (not in custom list)
        $this->assertFalse($config->isRetryable(new NexusApiException(429, 'Too Many')));

        // Should NOT retry 408 (not in custom list)
        $this->assertFalse($config->isRetryable(new NexusApiException(408, 'Timeout')));
    }
}
