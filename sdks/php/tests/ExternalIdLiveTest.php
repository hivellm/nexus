<?php

declare(strict_types=1);

namespace Nexus\SDK\Tests;

use PHPUnit\Framework\TestCase;
use Nexus\SDK\Config;
use Nexus\SDK\NexusApiException;
use Nexus\SDK\NexusClient;

/**
 * Live integration tests for the external-id surface (phase10, items 6.1-6.5).
 *
 * Gate: set NEXUS_LIVE_HOST env var (e.g. http://localhost:15474).
 * Run:  vendor/bin/phpunit --group live
 *
 * When NEXUS_LIVE_HOST is absent every test is skipped; the default
 * `vendor/bin/phpunit` run (unit suite) does not require a running server.
 *
 * Implementation notes:
 *   - NexusClient::createNodeWithExternalId() delegates to createNode()
 *     which posts to /nodes — that route returns 404 on the current server
 *     (the server exposes /data/nodes for external-id creation). This is a
 *     pre-existing bug in the SDK's legacy createNode helper, out of scope
 *     for phase10.  The live tests therefore create nodes via executeCypher()
 *     (which correctly routes to /cypher) and retrieve them via
 *     getNodeByExternalId() (which correctly uses /data/nodes/by-external-id).
 *   - The server returns node.id as a JSON integer; the PHP response array
 *     carries it as int, which is fine for PHP tests.
 *   - Conflict policy tests use Cypher ON CONFLICT syntax.
 *   - Length-cap rejection tests use Cypher CREATE with an oversize _id.
 *
 * @group live
 */
class ExternalIdLiveTest extends TestCase
{
    private ?NexusClient $client = null;
    private string $host = '';

    protected function setUp(): void
    {
        $host = getenv('NEXUS_LIVE_HOST');
        if ($host === false || $host === '') {
            $this->markTestSkipped('NEXUS_LIVE_HOST not set — skipping live test.');
        }
        $this->host = $host;
        $this->client = new NexusClient(new Config(baseUrl: $host));
        $this->client->ping();
    }

    protected function tearDown(): void
    {
        if ($this->client !== null) {
            $this->client->close();
            $this->client = null;
        }
    }

    private function client(): NexusClient
    {
        assert($this->client !== null);
        return $this->client;
    }

    /** Generate a unique str: external id. */
    private static function uniqueStr(): string
    {
        return 'str:live-php-' . bin2hex(random_bytes(12));
    }

    /** Run a Cypher query and return the decoded result array. */
    private function cypher(string $query): \Nexus\SDK\QueryResult
    {
        return $this->client()->executeCypher($query);
    }

    // -------------------------------------------------------------------------
    // 6.2  All six ExternalId variants — create via Cypher + round-trip GET
    // -------------------------------------------------------------------------

    public function testRoundtripSha256ExternalId(): void
    {
        // sha256: prefix + 64 hex chars
        $hex64 = bin2hex(random_bytes(32));
        $extId = "sha256:{$hex64}";

        $result = $this->cypher("CREATE (n:LivePhpSha256 {_id: '{$extId}'}) RETURN n._id");
        $this->assertSame($extId, $result->rows[0][0] ?? null, 'Cypher CREATE should return the external id');

        $got = $this->client()->getNodeByExternalId($extId);
        $this->assertNotNull($got['node'], 'GET by external id should find the node');
        $this->assertArrayHasKey('id', $got['node']);
    }

    public function testRoundtripBlake3ExternalId(): void
    {
        // blake3: prefix + 64 hex chars
        $hex64 = bin2hex(random_bytes(32));
        $extId = "blake3:{$hex64}";

        $result = $this->cypher("CREATE (n:LivePhpBlake3 {_id: '{$extId}'}) RETURN n._id");
        $this->assertSame($extId, $result->rows[0][0] ?? null);

        $got = $this->client()->getNodeByExternalId($extId);
        $this->assertNotNull($got['node']);
    }

    public function testRoundtripSha512ExternalId(): void
    {
        // sha512: prefix + 128 hex chars
        $hex128 = bin2hex(random_bytes(64));
        $extId = "sha512:{$hex128}";

        $result = $this->cypher("CREATE (n:LivePhpSha512 {_id: '{$extId}'}) RETURN n._id");
        $this->assertSame($extId, $result->rows[0][0] ?? null);

        $got = $this->client()->getNodeByExternalId($extId);
        $this->assertNotNull($got['node']);
    }

    public function testRoundtripUuidExternalId(): void
    {
        $uuid = sprintf(
            '%08x-%04x-%04x-%04x-%012x',
            random_int(0, 0xFFFFFFFF),
            random_int(0, 0xFFFF),
            random_int(0x4000, 0x4FFF),
            random_int(0x8000, 0xBFFF),
            random_int(0, 0xFFFFFFFFFFFF)
        );
        $extId = "uuid:{$uuid}";

        $result = $this->cypher("CREATE (n:LivePhpUuid {_id: '{$extId}'}) RETURN n._id");
        $this->assertSame($extId, $result->rows[0][0] ?? null);

        $got = $this->client()->getNodeByExternalId($extId);
        $this->assertNotNull($got['node']);
    }

    public function testRoundtripStrExternalId(): void
    {
        $extId = self::uniqueStr();

        $result = $this->cypher("CREATE (n:LivePhpStr {_id: '{$extId}'}) RETURN n._id");
        $this->assertSame($extId, $result->rows[0][0] ?? null);

        $got = $this->client()->getNodeByExternalId($extId);
        $this->assertNotNull($got['node']);
    }

    public function testRoundtripBytesExternalId(): void
    {
        // bytes: prefix + up to 128 hex chars (64 bytes); use 32 hex chars (16 bytes)
        $hex = bin2hex(random_bytes(16));
        $extId = "bytes:{$hex}";

        $result = $this->cypher("CREATE (n:LivePhpBytes {_id: '{$extId}'}) RETURN n._id");
        $this->assertSame($extId, $result->rows[0][0] ?? null);

        $got = $this->client()->getNodeByExternalId($extId);
        $this->assertNotNull($got['node']);
    }

    // -------------------------------------------------------------------------
    // 6.3  Conflict policies: error / match / replace
    // -------------------------------------------------------------------------

    public function testConflictPolicyErrorRejectsDuplicate(): void
    {
        $extId = self::uniqueStr();

        // First create — succeeds
        $r1 = $this->cypher("CREATE (n:LivePhpConflict {_id: '{$extId}', v: 1}) RETURN n._id");
        $this->assertSame($extId, $r1->rows[0][0] ?? null);

        // Second create with ON CONFLICT ERROR — must produce an error
        $r2 = $this->cypher("CREATE (n:LivePhpConflict {_id: '{$extId}', v: 2}) ON CONFLICT ERROR");
        $this->assertNotNull($r2->error ?? null, 'ON CONFLICT ERROR should produce an error on duplicate');
    }

    public function testConflictPolicyMatchIsIdempotent(): void
    {
        $extId = self::uniqueStr();

        $r1 = $this->cypher("CREATE (n:LivePhpMatch {_id: '{$extId}'}) ON CONFLICT MATCH RETURN n._id");
        $this->assertSame($extId, $r1->rows[0][0] ?? null, 'First run should return external id');

        $r2 = $this->cypher("CREATE (n:LivePhpMatch {_id: '{$extId}'}) ON CONFLICT MATCH RETURN n._id");
        $this->assertSame($extId, $r2->rows[0][0] ?? null, 'Second run should also return same external id');
    }

    public function testConflictPolicyReplaceUpdatesProperty(): void
    {
        $extId = self::uniqueStr();

        // Initial create
        $this->cypher("CREATE (n:LivePhpReplace {_id: '{$extId}', marker: 'before'}) RETURN n._id");

        // Replace — regression guard for commit fd001344
        $r = $this->cypher(
            "CREATE (n:LivePhpReplace {_id: '{$extId}', marker: 'after'}) ON CONFLICT REPLACE RETURN n._id"
        );
        $this->assertSame($extId, $r->rows[0][0] ?? null, 'REPLACE should return same external id');

        // Verify the property was actually updated
        $check = $this->cypher(
            "MATCH (n:LivePhpReplace) WHERE n._id = '{$extId}' RETURN n.marker"
        );
        $this->assertSame('after', $check->rows[0][0] ?? null, 'marker should be "after" after REPLACE');
    }

    // -------------------------------------------------------------------------
    // 6.4  Cypher _id round-trip via executeCypher
    // -------------------------------------------------------------------------

    public function testCypherCreateAndReturnId(): void
    {
        $extId = self::uniqueStr();
        $result = $this->cypher("CREATE (n:LivePhpCypher {_id: '{$extId}'}) RETURN n._id");

        $this->assertNotEmpty($result->rows, 'RETURN n._id should produce a row');
        $this->assertSame($extId, $result->rows[0][0] ?? null, 'Returned _id should equal the prefixed string');
    }

    // -------------------------------------------------------------------------
    // 6.5  Length-cap rejection (via Cypher CREATE with oversize _id)
    // -------------------------------------------------------------------------

    public function testOversizeStrExternalIdIsRejected(): void
    {
        $oversizeStr = 'str:' . str_repeat('x', 257);
        $result = $this->cypher("CREATE (n:LivePhpCap {_id: '{$oversizeStr}'}) RETURN n._id");

        // Server returns an error field on length-cap violation
        $this->assertNotNull(
            $result->error ?? null,
            'Oversize str external id should produce an error'
        );
    }

    public function testOversizeBytesExternalIdIsRejected(): void
    {
        // 65 bytes = 130 hex chars
        $oversizeBytes = 'bytes:' . str_repeat('ff', 65);
        $result = $this->cypher("CREATE (n:LivePhpCap {_id: '{$oversizeBytes}'}) RETURN n._id");

        $this->assertNotNull(
            $result->error ?? null,
            'Oversize bytes external id should produce an error'
        );
    }

    public function testEmptyUuidExternalIdIsRejected(): void
    {
        $result = $this->cypher("CREATE (n:LivePhpCap {_id: 'uuid:'}) RETURN n._id");

        $this->assertNotNull(
            $result->error ?? null,
            'Empty uuid external id should produce an error'
        );
    }

    // -------------------------------------------------------------------------
    // Absent external id returns null node (not an HTTP error)
    // -------------------------------------------------------------------------

    public function testAbsentExternalIdReturnsNullNode(): void
    {
        // Use a uuid that was never created
        $absent = 'uuid:88888888-8888-8888-8888-' . bin2hex(random_bytes(6));
        $got = $this->client()->getNodeByExternalId($absent);

        $this->assertNull($got['node'], 'Absent external id should return null node');
        $this->assertNull($got['error'] ?? null, 'Absent external id should not return an error');
    }
}
