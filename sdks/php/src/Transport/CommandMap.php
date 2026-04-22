<?php

declare(strict_types=1);

namespace Nexus\SDK\Transport;

/**
 * SDK dotted-name → wire-command mapping. Parity with
 * sdks/rust/src/transport/command_map.rs — 26 entries.
 */
final class CommandMap
{
    /**
     * @param array<string, mixed> $payload
     * @return array{command: string, args: NexusValue[]}|null
     */
    public static function map(string $dotted, array $payload): ?array
    {
        switch ($dotted) {
            case 'graph.cypher':
                if (!isset($payload['query']) || !is_string($payload['query'])) {
                    return null;
                }
                $args = [NexusValue::str($payload['query'])];
                if (array_key_exists('parameters', $payload) && $payload['parameters'] !== null) {
                    $args[] = self::jsonToNexus($payload['parameters']);
                }
                return ['command' => 'CYPHER', 'args' => $args];
            case 'graph.ping':
                return ['command' => 'PING', 'args' => []];
            case 'graph.hello':
                return ['command' => 'HELLO', 'args' => [NexusValue::int(1)]];
            case 'graph.stats':
                return ['command' => 'STATS', 'args' => []];
            case 'graph.health':
                return ['command' => 'HEALTH', 'args' => []];
            case 'graph.quit':
                return ['command' => 'QUIT', 'args' => []];
            case 'auth.login':
                $apiKey = $payload['api_key'] ?? null;
                if (is_string($apiKey) && $apiKey !== '') {
                    return ['command' => 'AUTH', 'args' => [NexusValue::str($apiKey)]];
                }
                $u = $payload['username'] ?? null;
                $p = $payload['password'] ?? null;
                if (!is_string($u) || !is_string($p)) {
                    return null;
                }
                return ['command' => 'AUTH', 'args' => [NexusValue::str($u), NexusValue::str($p)]];
            case 'db.list':
                return ['command' => 'DB_LIST', 'args' => []];
            case 'db.create':
            case 'db.drop':
            case 'db.use':
                $n = $payload['name'] ?? null;
                if (!is_string($n)) {
                    return null;
                }
                $cmd = match ($dotted) {
                    'db.create' => 'DB_CREATE',
                    'db.drop' => 'DB_DROP',
                    'db.use' => 'DB_USE',
                };
                return ['command' => $cmd, 'args' => [NexusValue::str($n)]];
            case 'schema.labels':
                return ['command' => 'LABELS', 'args' => []];
            case 'schema.rel_types':
                return ['command' => 'REL_TYPES', 'args' => []];
            case 'schema.property_keys':
                return ['command' => 'PROPERTY_KEYS', 'args' => []];
            case 'schema.indexes':
                return ['command' => 'INDEXES', 'args' => []];
            case 'data.export':
                $fmt = $payload['format'] ?? null;
                if (!is_string($fmt)) {
                    return null;
                }
                $args = [NexusValue::str($fmt)];
                $q = $payload['query'] ?? null;
                if (is_string($q)) {
                    $args[] = NexusValue::str($q);
                }
                return ['command' => 'EXPORT', 'args' => $args];
            case 'data.import':
                $fmt = $payload['format'] ?? null;
                $d = $payload['data'] ?? null;
                if (!is_string($fmt) || !is_string($d)) {
                    return null;
                }
                return ['command' => 'IMPORT', 'args' => [NexusValue::str($fmt), NexusValue::str($d)]];
        }
        return null;
    }

    public static function jsonToNexus(mixed $v): NexusValue
    {
        if ($v === null) {
            return NexusValue::null();
        }
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
        if (!is_array($v)) {
            return NexusValue::null();
        }
        // List vs assoc.
        if (array_keys($v) === range(0, count($v) - 1)) {
            return NexusValue::array(array_map(
                fn ($e) => self::jsonToNexus($e),
                $v,
            ));
        }
        $pairs = [];
        foreach ($v as $k => $val) {
            $pairs[] = [NexusValue::str((string) $k), self::jsonToNexus($val)];
        }
        return NexusValue::map($pairs);
    }

    public static function nexusToJson(NexusValue $v): mixed
    {
        return match ($v->kind) {
            NexusValueKind::Null => null,
            NexusValueKind::Bool,
            NexusValueKind::Int,
            NexusValueKind::Float,
            NexusValueKind::Str,
            NexusValueKind::Bytes => $v->value,
            NexusValueKind::Array => array_map(
                fn (NexusValue $e) => self::nexusToJson($e),
                $v->value,
            ),
            NexusValueKind::Map => self::mapToAssoc($v->value),
        };
    }

    /**
     * @param array<int, array{0: NexusValue, 1: NexusValue}> $pairs
     * @return array<string, mixed>
     */
    private static function mapToAssoc(array $pairs): array
    {
        $out = [];
        foreach ($pairs as $pair) {
            $key = $pair[0]->kind === NexusValueKind::Str
                ? (string) $pair[0]->value
                : (string) $pair[0]->value;
            $out[$key] = self::nexusToJson($pair[1]);
        }
        return $out;
    }
}
