<?php

declare(strict_types=1);

namespace Nexus\SDK\Transport;

use MessagePack\MessagePack;

/**
 * Wire codec for the Nexus RPC protocol.
 *
 * The server-side types live in nexus_protocol::rpc::types. rmp-serde
 * encodes Rust enums using the externally-tagged representation by
 * default:
 *
 *   - Unit variants (NexusValue::Null) → the string "Null".
 *   - Data-bearing variants (NexusValue::Str("hi")) → {"Str": "hi"}.
 *   - Result<T, E> → {"Ok": v} / {"Err": s}.
 *
 * toWire / fromWire translate between NexusValue and the on-wire
 * shape. encodeRequestFrame / decodeResponseBody handle the
 * `u32 LE length prefix + MessagePack body` frame format.
 */
final class Codec
{
    public static function toWire(NexusValue $v): mixed
    {
        return match ($v->kind) {
            NexusValueKind::Null => 'Null',
            NexusValueKind::Bool => ['Bool' => (bool) $v->value],
            NexusValueKind::Int => ['Int' => (int) $v->value],
            NexusValueKind::Float => ['Float' => (float) $v->value],
            NexusValueKind::Bytes => ['Bytes' => (string) $v->value],
            NexusValueKind::Str => ['Str' => (string) $v->value],
            NexusValueKind::Array => ['Array' => array_map(
                fn (NexusValue $e) => self::toWire($e),
                $v->value,
            )],
            NexusValueKind::Map => ['Map' => array_map(
                fn (array $pair) => [self::toWire($pair[0]), self::toWire($pair[1])],
                $v->value,
            )],
        };
    }

    public static function fromWire(mixed $raw): NexusValue
    {
        if ($raw === null) {
            return NexusValue::null();
        }
        if ($raw === 'Null') {
            return NexusValue::null();
        }
        if (is_bool($raw)) {
            return NexusValue::bool($raw);
        }
        if (is_int($raw)) {
            return NexusValue::int($raw);
        }
        if (is_float($raw)) {
            return NexusValue::float($raw);
        }
        if (is_string($raw)) {
            return NexusValue::str($raw);
        }
        if (!is_array($raw)) {
            throw new \RuntimeException(
                sprintf('decode: unexpected NexusValue wire type %s', get_debug_type($raw)),
            );
        }

        // Tagged map (associative array with a single key).
        if (self::isAssoc($raw)) {
            if (count($raw) !== 1) {
                throw new \RuntimeException(sprintf(
                    'decode: expected single-key tagged NexusValue, got %d keys',
                    count($raw),
                ));
            }
            $tag = array_key_first($raw);
            /** @var mixed $payload */
            $payload = $raw[$tag];
            return self::fromTagged((string) $tag, $payload);
        }

        // Plain list — treat as Array variant.
        $out = [];
        foreach ($raw as $e) {
            $out[] = self::fromWire($e);
        }
        return NexusValue::array($out);
    }

    /**
     * @param array<mixed> $arr
     */
    private static function isAssoc(array $arr): bool
    {
        return array_keys($arr) !== range(0, count($arr) - 1);
    }

    private static function fromTagged(string $tag, mixed $payload): NexusValue
    {
        switch ($tag) {
            case 'Null':
                return NexusValue::null();
            case 'Bool':
                return NexusValue::bool((bool) $payload);
            case 'Int':
                if (!is_int($payload) && !is_float($payload)) {
                    throw new \RuntimeException('decode: Int payload must be numeric');
                }
                return NexusValue::int((int) $payload);
            case 'Float':
                if (!is_int($payload) && !is_float($payload)) {
                    throw new \RuntimeException('decode: Float payload must be numeric');
                }
                return NexusValue::float((float) $payload);
            case 'Bytes':
                if (!is_string($payload)) {
                    throw new \RuntimeException('decode: Bytes payload must be a string');
                }
                return NexusValue::bytes($payload);
            case 'Str':
                return NexusValue::str((string) $payload);
            case 'Array':
                if (!is_array($payload)) {
                    throw new \RuntimeException('decode: Array payload must be array');
                }
                $out = [];
                foreach ($payload as $e) {
                    $out[] = self::fromWire($e);
                }
                return NexusValue::array($out);
            case 'Map':
                if (!is_array($payload)) {
                    throw new \RuntimeException('decode: Map payload must be array');
                }
                $pairs = [];
                foreach ($payload as $pair) {
                    if (!is_array($pair) || count($pair) !== 2) {
                        throw new \RuntimeException('decode: Map entry must be [key, value] pair');
                    }
                    $values = array_values($pair);
                    $pairs[] = [self::fromWire($values[0]), self::fromWire($values[1])];
                }
                return NexusValue::map($pairs);
        }
        throw new \RuntimeException(sprintf("decode: unknown NexusValue tag '%s'", $tag));
    }

    /**
     * Encode a request into a length-prefixed MessagePack frame.
     * Layout: u32_le(body_len) ++ msgpack(body).
     *
     * @param NexusValue[] $args
     */
    public static function encodeRequestFrame(int $id, string $command, array $args): string
    {
        $body = MessagePack::pack([
            'id' => $id,
            'command' => $command,
            'args' => array_map(fn (NexusValue $a) => self::toWire($a), $args),
        ]);
        return pack('V', strlen($body)) . $body;
    }

    /**
     * Decode a response body (MessagePack bytes AFTER the length prefix).
     *
     * @return array{id: int, ok: bool, value: ?NexusValue, err: string}
     */
    public static function decodeResponseBody(string $body): array
    {
        /** @var mixed $raw */
        $raw = MessagePack::unpack($body);
        if (!is_array($raw)) {
            throw new \RuntimeException(
                sprintf('decode: response must be a map, got %s', get_debug_type($raw)),
            );
        }
        if (!array_key_exists('id', $raw)) {
            throw new \RuntimeException('decode: response missing id');
        }
        $id = (int) $raw['id'];
        if (!array_key_exists('result', $raw) || !is_array($raw['result'])) {
            throw new \RuntimeException('decode: response missing result');
        }
        /** @var array<string, mixed> $result */
        $result = $raw['result'];
        if (count($result) !== 1) {
            throw new \RuntimeException('decode: Result must be a single-key tagged map');
        }
        $tag = (string) array_key_first($result);
        $payload = $result[$tag];
        return match ($tag) {
            'Ok' => ['id' => $id, 'ok' => true, 'value' => self::fromWire($payload), 'err' => ''],
            'Err' => ['id' => $id, 'ok' => false, 'value' => null, 'err' => (string) $payload],
            default => throw new \RuntimeException(sprintf("decode: Result must be 'Ok' or 'Err', got '%s'", $tag)),
        };
    }
}
