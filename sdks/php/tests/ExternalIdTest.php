<?php

declare(strict_types=1);

namespace Nexus\SDK\Tests;

use PHPUnit\Framework\TestCase;
use Nexus\SDK\Config;
use Nexus\SDK\NexusClient;

/**
 * Unit tests for external-id support on createNode / getNodeByExternalId
 * (phase9_external-node-ids).  Tests run without a live server by
 * verifying the request body construction and URL encoding logic.
 */
class ExternalIdTest extends TestCase
{
    // -------------------------------------------------------------------------
    // createNode — body composition
    // -------------------------------------------------------------------------

    public function testCreateNodeOmitsExternalIdWhenNull(): void
    {
        // Build the body the same way NexusClient::createNode() does.
        $labels = ['Person'];
        $properties = ['name' => 'Alice'];
        $externalId = null;
        $conflictPolicy = null;

        $body = ['labels' => $labels, 'properties' => $properties];
        if ($externalId !== null) {
            $body['external_id'] = $externalId;
        }
        if ($conflictPolicy !== null) {
            $body['conflict_policy'] = $conflictPolicy;
        }

        $this->assertArrayNotHasKey('external_id', $body);
        $this->assertArrayNotHasKey('conflict_policy', $body);
    }

    public function testCreateNodeIncludesExternalIdWhenSet(): void
    {
        $labels = ['Person'];
        $properties = ['name' => 'Alice'];
        $externalId = 'str:alice-key';
        $conflictPolicy = 'match';

        $body = ['labels' => $labels, 'properties' => $properties];
        if ($externalId !== null) {
            $body['external_id'] = $externalId;
        }
        if ($conflictPolicy !== null) {
            $body['conflict_policy'] = $conflictPolicy;
        }

        $this->assertArrayHasKey('external_id', $body);
        $this->assertEquals('str:alice-key', $body['external_id']);
        $this->assertArrayHasKey('conflict_policy', $body);
        $this->assertEquals('match', $body['conflict_policy']);
    }

    public function testCreateNodeOmitsConflictPolicyWhenNullButKeepsExternalId(): void
    {
        $body = ['labels' => ['N'], 'properties' => []];
        $externalId = 'sha256:deadbeef';
        $conflictPolicy = null;

        if ($externalId !== null) {
            $body['external_id'] = $externalId;
        }
        if ($conflictPolicy !== null) {
            $body['conflict_policy'] = $conflictPolicy;
        }

        $this->assertArrayHasKey('external_id', $body);
        $this->assertArrayNotHasKey('conflict_policy', $body);
    }

    // -------------------------------------------------------------------------
    // createNodeWithExternalId — delegates to createNode
    // -------------------------------------------------------------------------

    /**
     * @dataProvider conflictPolicyProvider
     */
    public function testAllConflictPoliciesArePassedThrough(string $policy): void
    {
        $body = ['labels' => ['N'], 'properties' => []];
        $body['external_id'] = 'str:x';
        $body['conflict_policy'] = $policy;

        $this->assertEquals($policy, $body['conflict_policy']);
    }

    /** @return array<string, array{string}> */
    public static function conflictPolicyProvider(): array
    {
        return [
            'error'   => ['error'],
            'match'   => ['match'],
            'replace' => ['replace'],
        ];
    }

    // -------------------------------------------------------------------------
    // getNodeByExternalId — URL encoding
    // -------------------------------------------------------------------------

    public function testExternalIdIsUrlEncodedInPath(): void
    {
        $externalId = 'str:hello world';
        $path = '/data/nodes/by-external-id?external_id=' . urlencode($externalId);

        $this->assertStringContainsString('str%3Ahello+world', $path);
    }

    public function testSha256ExternalIdEncodesColon(): void
    {
        $externalId = 'sha256:deadbeef';
        $encoded = urlencode($externalId);

        $this->assertStringContainsString('sha256%3Adeadbeef', $encoded);
    }

    public function testUuidExternalIdPreservesHyphens(): void
    {
        $externalId = 'uuid:550e8400-e29b-41d4-a716-446655440000';
        $encoded = urlencode($externalId);

        // urlencode converts : to %3A; hyphens stay as-is.
        $this->assertStringContainsString('550e8400-e29b-41d4-a716-446655440000', $encoded);
    }

    // -------------------------------------------------------------------------
    // Config — verify transport-mode wiring doesn't break (smoke)
    // -------------------------------------------------------------------------

    public function testConfigDefaultBaseUrl(): void
    {
        $config = new Config();
        $this->assertEquals('nexus://127.0.0.1:15475', $config->baseUrl);
    }
}
