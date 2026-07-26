<?php

declare(strict_types=1);

/**
 * Interop cell: PHP SDK (hivellm/thunder) RpcTransport against a Thunder-based
 * Nexus server.
 *
 * Drives RpcTransport directly rather than the higher-level client -- the
 * matrix is about the wire, and the transport is where the wire lives.
 *
 *     argv:   <host> <port> <user> <pass>
 *     stdout: one `STEP <name> PASS|FAIL <detail>` line per step
 *     exit:   0 iff every step passed
 */

require 'e:/HiveLLM/Nexus/sdks/php/vendor/autoload.php';

use Nexus\SDK\Transport\Credentials;
use Nexus\SDK\Transport\Endpoint;
use Nexus\SDK\Transport\NexusValue;
use Nexus\SDK\Transport\NexusValueKind;
use Nexus\SDK\Transport\RpcTransport;

/** A vector whose f32-LE encoding is not valid UTF-8, so a transport that
 * quietly round-trips Bytes through a string cannot pass the knn_bytes cell. */
const VEC = [1.5, -2.5, 3.5, INF];

function report(string $step, bool $ok, string $detail): void
{
    printf("STEP %s %s %s\n", $step, $ok ? 'PASS' : 'FAIL', $detail);
    flush();
}

function boolStr(bool $v): string
{
    return $v ? 'true' : 'false';
}

/** Look up a string key in a Map-kind NexusValue. */
function mapGet(NexusValue $v, string $key): ?NexusValue
{
    if ($v->kind !== NexusValueKind::Map) {
        return null;
    }
    foreach ($v->value as $pair) {
        [$k, $val] = $pair;
        if ($k->kind === NexusValueKind::Str && $k->value === $key) {
            return $val;
        }
    }
    return null;
}

function toNx(mixed $v): NexusValue
{
    if (is_bool($v)) {
        return NexusValue::bool($v);
    }
    if (is_int($v)) {
        return NexusValue::int($v);
    }
    if (is_float($v)) {
        return NexusValue::float($v);
    }
    if (is_string($v)) {
        return NexusValue::str($v);
    }
    return NexusValue::str((string) $v);
}

/**
 * Run CYPHER over the transport and return rows as native PHP arrays.
 *
 * @param array<string, mixed> $params
 * @return list<list<mixed>>
 */
function cypherRows(RpcTransport $t, string $query, array $params): array
{
    $args = [NexusValue::str($query)];
    if ($params !== []) {
        $pairs = [];
        foreach ($params as $k => $v) {
            $pairs[] = [NexusValue::str((string) $k), toNx($v)];
        }
        $args[] = NexusValue::map($pairs);
    }

    $resp = $t->execute('CYPHER', $args);
    $err = mapGet($resp, 'error');
    if ($err !== null && $err->kind === NexusValueKind::Str && $err->value !== '' && $err->value !== null) {
        throw new \RuntimeException((string) $err->value);
    }

    $rows = mapGet($resp, 'rows');
    if ($rows === null || $rows->kind !== NexusValueKind::Array) {
        return [];
    }
    $out = [];
    foreach ($rows->value as $row) {
        if ($row->kind !== NexusValueKind::Array) {
            continue;
        }
        $out[] = array_map(static fn (NexusValue $cell) => $cell->value, $row->value);
    }
    return $out;
}

/**
 * @param list<string> $argv
 */
function main(array $argv): int
{
    $host = $argv[1];
    $port = (int) $argv[2];
    $user = $argv[3];
    $password = $argv[4];
    $endpoint = new Endpoint('nexus', $host, $port);
    $failures = 0;

    // 1. auth -- PING answers before auth; STATS is refused before AUTH and
    //    succeeds after. An unauthenticated transport is one built with no
    //    credentials at all.
    $authed = null;
    try {
        $anon = new RpcTransport($endpoint, new Credentials());
        $pong = $anon->execute('PING', []);
        $pingOk = $pong->kind === NexusValueKind::Str && $pong->value === 'PONG';

        $statsPreRefused = false;
        try {
            $anon->execute('STATS', []);
        } catch (\Throwable $exc) {
            // Any typed refusal is fine; every convention lands on "auth"
            // somewhere in the message (NOAUTH, WRONGPASS, ...).
            $statsPreRefused = str_contains(strtolower($exc->getMessage()), 'auth');
        }
        $anon->close();

        $authed = new RpcTransport($endpoint, new Credentials(username: $user, password: $password));
        $stats = $authed->execute('STATS', []);
        $statsPostOk = in_array($stats->kind, [NexusValueKind::Map, NexusValueKind::Str], true);

        $ok = $pingOk && $statsPreRefused && $statsPostOk;
        report('auth', $ok, sprintf(
            'ping=%s stats_pre_refused=%s stats_post=%s',
            boolStr($pingOk),
            boolStr($statsPreRefused),
            boolStr($statsPostOk),
        ));
        if (!$ok) {
            $failures++;
        }
    } catch (\Throwable $exc) {
        report('auth', false, sprintf('%s: %s', get_class($exc), $exc->getMessage()));
        return 1;
    }

    // 2. cypher -- CREATE then MATCH round-trips the id back.
    $marker = 424205;
    try {
        cypherRows($authed, 'CREATE (n:InteropPhp {id: $id}) RETURN n.id', ['id' => $marker]);
        $rows = cypherRows($authed, 'MATCH (n:InteropPhp {id: $id}) RETURN n.id', ['id' => $marker]);
        $got = ($rows !== [] && $rows[0] !== []) ? $rows[0][0] : null;
        $ok = $got !== null && (int) $got === $marker;
        report('cypher', $ok, sprintf('round-trip id -> %s', var_export($got, true)));
        if (!$ok) {
            $failures++;
        }
    } catch (\Throwable $exc) {
        report('cypher', false, sprintf('%s: %s', get_class($exc), $exc->getMessage()));
        $failures++;
    }

    // 3. knn_bytes -- a raw f32-LE vector carried as Bytes round-trips
    //    byte-exact (PING echoes its argument). `pack('g', ...)` is float,
    //    little-endian, 4 bytes -- exactly struct.pack("<f", x) in Python.
    $vecBytes = '';
    foreach (VEC as $x) {
        $vecBytes .= pack('g', $x);
    }
    try {
        $echoed = $authed->execute('PING', [NexusValue::bytes($vecBytes)]);
        $got = $echoed->kind === NexusValueKind::Bytes ? (string) $echoed->value : '';
        $ok = $got === $vecBytes && strlen($vecBytes) === 4 * count(VEC);
        report('knn_bytes', $ok, sprintf('%s -> %s', bin2hex($vecBytes), bin2hex($got)));
        if (!$ok) {
            $failures++;
        }
    } catch (\Throwable $exc) {
        report('knn_bytes', false, sprintf('%s: %s', get_class($exc), $exc->getMessage()));
        $failures++;
    }

    // 4. error -- a broken CYPHER surfaces a typed server error, not a
    //    transport crash, and the same connection stays usable.
    try {
        try {
            cypherRows($authed, 'MATCH (n RETURN', []);
            report('error', false, 'expected a server error, got a result');
            $failures++;
        } catch (\Throwable $exc) {
            $pong = $authed->execute('PING', []);
            $alive = $pong->kind === NexusValueKind::Str && $pong->value === 'PONG';
            report('error', $alive, sprintf('raised %s; connection alive=%s', get_class($exc), boolStr($alive)));
            if (!$alive) {
                $failures++;
            }
        }
    } finally {
        $authed->close();
    }

    return $failures > 0 ? 1 : 0;
}

exit(main($argv));
