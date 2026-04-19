<?php

declare(strict_types=1);

namespace Nexus\SDK\Transport;

/**
 * Discriminator for NexusValue.
 */
enum NexusValueKind: string
{
    case Null = 'Null';
    case Bool = 'Bool';
    case Int = 'Int';
    case Float = 'Float';
    case Bytes = 'Bytes';
    case Str = 'Str';
    case Array = 'Array';
    case Map = 'Map';
}

/**
 * Dynamically-typed value carried by RPC requests and responses.
 *
 * Mirrors nexus_protocol::rpc::types::NexusValue — a tagged union
 * rather than an untyped `mixed`, so SDKs can map every wire variant
 * to a native PHP value with a single `match` on the kind.
 */
final class NexusValue
{
    public function __construct(
        public readonly NexusValueKind $kind,
        public readonly mixed $value = null,
    ) {
    }

    public static function null(): self
    {
        return new self(NexusValueKind::Null);
    }

    public static function bool(bool $v): self
    {
        return new self(NexusValueKind::Bool, $v);
    }

    public static function int(int $v): self
    {
        return new self(NexusValueKind::Int, $v);
    }

    public static function float(float $v): self
    {
        return new self(NexusValueKind::Float, $v);
    }

    public static function bytes(string $v): self
    {
        return new self(NexusValueKind::Bytes, $v);
    }

    public static function str(string $v): self
    {
        return new self(NexusValueKind::Str, $v);
    }

    /**
     * @param NexusValue[] $v
     */
    public static function array(array $v): self
    {
        return new self(NexusValueKind::Array, $v);
    }

    /**
     * @param array<int, array{0: NexusValue, 1: NexusValue}> $pairs
     */
    public static function map(array $pairs): self
    {
        return new self(NexusValueKind::Map, $pairs);
    }

    public function asString(): ?string
    {
        return $this->kind === NexusValueKind::Str && is_string($this->value)
            ? $this->value
            : null;
    }

    public function asInt(): ?int
    {
        return $this->kind === NexusValueKind::Int && is_int($this->value)
            ? $this->value
            : null;
    }
}
