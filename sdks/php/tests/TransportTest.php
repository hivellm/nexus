<?php

declare(strict_types=1);

namespace Nexus\SDK\Tests;

use Nexus\SDK\Transport\Codec;
use Nexus\SDK\Transport\CommandMap;
use Nexus\SDK\Transport\Credentials;
use Nexus\SDK\Transport\Endpoint;
use Nexus\SDK\Transport\NexusValue;
use Nexus\SDK\Transport\NexusValueKind;
use Nexus\SDK\Transport\TransportFactory;
use Nexus\SDK\Transport\TransportMode;
use PHPUnit\Framework\TestCase;

final class TransportTest extends TestCase
{
    // ── Endpoint parser ────────────────────────────────────────────

    public function testDefaultLocalIsNexusLoopback(): void
    {
        $ep = Endpoint::defaultLocal();
        $this->assertSame('nexus', $ep->scheme);
        $this->assertSame('127.0.0.1', $ep->host);
        $this->assertSame(Endpoint::RPC_DEFAULT_PORT, $ep->port);
        $this->assertSame('nexus://127.0.0.1:15475', (string) $ep);
    }

    public function testParseNexusWithExplicitPort(): void
    {
        $ep = Endpoint::parse('nexus://example.com:17000');
        $this->assertSame('nexus', $ep->scheme);
        $this->assertSame(17000, $ep->port);
    }

    public function testParseHttpDefaultPort(): void
    {
        $ep = Endpoint::parse('http://localhost');
        $this->assertSame('http', $ep->scheme);
        $this->assertSame(Endpoint::HTTP_DEFAULT_PORT, $ep->port);
    }

    public function testParseHttpsDefaultPort(): void
    {
        $ep = Endpoint::parse('https://nexus.example.com');
        $this->assertSame('https', $ep->scheme);
        $this->assertSame(Endpoint::HTTPS_DEFAULT_PORT, $ep->port);
    }

    public function testParseBareIsRpc(): void
    {
        $ep = Endpoint::parse('10.0.0.5:15600');
        $this->assertSame('nexus', $ep->scheme);
        $this->assertSame(15600, $ep->port);
    }

    public function testParseIPv6(): void
    {
        $ep = Endpoint::parse('nexus://[::1]:15475');
        $this->assertSame('::1', $ep->host);
        $this->assertSame(15475, $ep->port);
    }

    public function testRejectsNexusRpcScheme(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessageMatches('/unsupported URL scheme/');
        Endpoint::parse('nexus-rpc://host');
    }

    public function testRejectsEmpty(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        Endpoint::parse('');
    }

    public function testAsHttpUrlSwapsRpcToSiblingPort(): void
    {
        $ep = Endpoint::parse('nexus://host:17000');
        $this->assertSame('http://host:15474', $ep->asHttpUrl());
    }

    // ── Wire codec ────────────────────────────────────────────────

    public function testEncodesNullAsLiteralString(): void
    {
        $this->assertSame('Null', Codec::toWire(NexusValue::null()));
    }

    public function testEncodesStrAsTaggedMap(): void
    {
        $this->assertSame(['Str' => 'hi'], Codec::toWire(NexusValue::str('hi')));
    }

    public function testRoundtripsPrimitives(): void
    {
        $cases = [
            NexusValue::null(),
            NexusValue::bool(true),
            NexusValue::bool(false),
            NexusValue::int(0),
            NexusValue::int(-42),
            NexusValue::str(''),
            NexusValue::str('hello'),
            NexusValue::float(1.5),
        ];
        foreach ($cases as $v) {
            $back = Codec::fromWire(Codec::toWire($v));
            $this->assertSame($v->kind, $back->kind);
        }
    }

    public function testRoundtripsNestedArrayAndMap(): void
    {
        $v = NexusValue::map([
            [NexusValue::str('labels'), NexusValue::array([NexusValue::str('Person')])],
            [NexusValue::str('age'), NexusValue::int(30)],
        ]);
        $back = Codec::fromWire(Codec::toWire($v));
        $this->assertSame(NexusValueKind::Map, $back->kind);
    }

    public function testRejectsMultiKeyTaggedValue(): void
    {
        $this->expectException(\RuntimeException::class);
        $this->expectExceptionMessageMatches('/single-key/');
        Codec::fromWire(['Str' => 'a', 'Int' => 1]);
    }

    public function testRejectsUnknownTag(): void
    {
        $this->expectException(\RuntimeException::class);
        $this->expectExceptionMessageMatches('/unknown NexusValue tag/');
        Codec::fromWire(['Widget' => 'x']);
    }

    public function testRequestFrameHasU32LeLengthPrefix(): void
    {
        $frame = Codec::encodeRequestFrame(7, 'PING', []);
        /** @var array{1: int} $unpacked */
        $unpacked = unpack('V', substr($frame, 0, 4));
        $this->assertSame(strlen($frame) - 4, $unpacked[1]);
        $this->assertGreaterThan(0, $unpacked[1]);
    }

    // ── Command map ───────────────────────────────────────────────

    public function testCypherSimple(): void
    {
        $m = CommandMap::map('graph.cypher', ['query' => 'RETURN 1']);
        $this->assertNotNull($m);
        $this->assertSame('CYPHER', $m['command']);
        $this->assertCount(1, $m['args']);
        $this->assertSame('RETURN 1', $m['args'][0]->asString());
    }

    public function testCypherWithParams(): void
    {
        $m = CommandMap::map('graph.cypher', [
            'query' => 'MATCH (n {name:$n}) RETURN n',
            'parameters' => ['n' => 'Alice'],
        ]);
        $this->assertNotNull($m);
        $this->assertCount(2, $m['args']);
        $this->assertSame(NexusValueKind::Map, $m['args'][1]->kind);
    }

    public function testNoArgVerbs(): void
    {
        foreach (['graph.ping', 'graph.stats', 'graph.health', 'graph.quit'] as $name) {
            $m = CommandMap::map($name, []);
            $this->assertNotNull($m, "$name");
            $this->assertEmpty($m['args']);
        }
    }

    public function testAuthApiKeyWins(): void
    {
        $m = CommandMap::map('auth.login', [
            'api_key' => 'nx_1',
            'username' => 'u',
            'password' => 'p',
        ]);
        $this->assertNotNull($m);
        $this->assertCount(1, $m['args']);
        $this->assertSame('nx_1', $m['args'][0]->asString());
    }

    public function testAuthFallsBackToUserPass(): void
    {
        $m = CommandMap::map('auth.login', ['username' => 'u', 'password' => 'p']);
        $this->assertNotNull($m);
        $this->assertCount(2, $m['args']);
    }

    public function testDbCreateRequiresName(): void
    {
        $this->assertNull(CommandMap::map('db.create', []));
        $m = CommandMap::map('db.create', ['name' => 'mydb']);
        $this->assertNotNull($m);
        $this->assertSame('DB_CREATE', $m['command']);
    }

    public function testDataImportRequiresBoth(): void
    {
        $this->assertNull(CommandMap::map('data.import', ['format' => 'json']));
        $this->assertNull(CommandMap::map('data.import', ['data' => '[]']));
        $m = CommandMap::map('data.import', ['format' => 'json', 'data' => '[]']);
        $this->assertNotNull($m);
    }

    public function testUnknownReturnsNull(): void
    {
        $this->assertNull(CommandMap::map('graph.nonsense', []));
    }

    // ── TransportMode parse ───────────────────────────────────────

    public function testModeCanonicalTokens(): void
    {
        $this->assertSame(TransportMode::NexusRpc, TransportMode::parse('nexus'));
        $this->assertSame(TransportMode::Http, TransportMode::parse('http'));
        $this->assertSame(TransportMode::Https, TransportMode::parse('https'));
        $this->assertSame(TransportMode::Resp3, TransportMode::parse('resp3'));
    }

    public function testModeAliases(): void
    {
        $this->assertSame(TransportMode::NexusRpc, TransportMode::parse('rpc'));
        $this->assertSame(TransportMode::NexusRpc, TransportMode::parse('NexusRpc'));
        $this->assertSame(TransportMode::NexusRpc, TransportMode::parse('NEXUSRPC'));
    }

    public function testModeAutoAndEmpty(): void
    {
        $this->assertNull(TransportMode::parse(''));
        $this->assertNull(TransportMode::parse('auto'));
        $this->assertNull(TransportMode::parse('widget'));
    }

    // ── TransportFactory precedence ───────────────────────────────

    public function testDefaultIsRpc(): void
    {
        $built = TransportFactory::build(null, new Credentials(), envTransport: '');
        $this->assertSame(TransportMode::NexusRpc, $built['mode']);
        $this->assertSame(Endpoint::RPC_DEFAULT_PORT, $built['endpoint']->port);
        $built['transport']->close();
    }

    public function testUrlSchemeWinsOverEnv(): void
    {
        $built = TransportFactory::build(
            'http://host:15474',
            new Credentials(),
            envTransport: 'nexus',
        );
        $this->assertSame(TransportMode::Http, $built['mode']);
        $built['transport']->close();
    }

    public function testEnvOverridesBareHost(): void
    {
        $built = TransportFactory::build(
            'host:15474',
            new Credentials(),
            envTransport: 'http',
        );
        $this->assertSame(TransportMode::Http, $built['mode']);
        $built['transport']->close();
    }

    public function testResp3ThrowsClearError(): void
    {
        $this->expectException(\InvalidArgumentException::class);
        $this->expectExceptionMessageMatches('/resp3 transport is not yet shipped/');
        TransportFactory::build(
            null,
            new Credentials(),
            transportHint: TransportMode::Resp3,
            envTransport: '',
        );
    }

    // ── Credentials ───────────────────────────────────────────────

    public function testCredentialsEmpty(): void
    {
        $this->assertFalse((new Credentials())->hasAny());
    }

    public function testCredentialsApiKey(): void
    {
        $this->assertTrue((new Credentials(apiKey: 'k'))->hasAny());
    }

    public function testCredentialsUsernameAlone(): void
    {
        $this->assertFalse((new Credentials(username: 'u'))->hasAny());
    }

    public function testCredentialsUserAndPass(): void
    {
        $this->assertTrue((new Credentials(username: 'u', password: 'p'))->hasAny());
    }
}
