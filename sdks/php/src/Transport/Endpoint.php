<?php

declare(strict_types=1);

namespace Nexus\SDK\Transport;

/**
 * A parsed endpoint URL. Mirrors nexus-cli/src/endpoint.rs.
 */
final class Endpoint
{
    public const RPC_DEFAULT_PORT = 15475;
    public const HTTP_DEFAULT_PORT = 15474;
    public const HTTPS_DEFAULT_PORT = 443;
    public const RESP3_DEFAULT_PORT = 15476;

    public function __construct(
        public readonly string $scheme,
        public readonly string $host,
        public readonly int $port,
    ) {
    }

    public static function defaultLocal(): self
    {
        return new self('nexus', '127.0.0.1', self::RPC_DEFAULT_PORT);
    }

    public function authority(): string
    {
        return $this->host . ':' . $this->port;
    }

    public function __toString(): string
    {
        return $this->scheme . '://' . $this->authority();
    }

    public function asHttpUrl(): string
    {
        return match ($this->scheme) {
            'http' => 'http://' . $this->authority(),
            'https' => 'https://' . $this->authority(),
            default => 'http://' . $this->host . ':' . self::HTTP_DEFAULT_PORT,
        };
    }

    public function isRpc(): bool
    {
        return $this->scheme === 'nexus';
    }

    public static function parse(string $raw): self
    {
        $trimmed = trim($raw);
        if ($trimmed === '') {
            throw new \InvalidArgumentException('endpoint URL must not be empty');
        }

        $sep = strpos($trimmed, '://');
        if ($sep !== false) {
            $schemeRaw = strtolower(substr($trimmed, 0, $sep));
            $rest = rtrim(substr($trimmed, $sep + 3), '/');
            [$scheme, $defaultPort] = match ($schemeRaw) {
                'nexus' => ['nexus', self::RPC_DEFAULT_PORT],
                'http' => ['http', self::HTTP_DEFAULT_PORT],
                'https' => ['https', self::HTTPS_DEFAULT_PORT],
                'resp3' => ['resp3', self::RESP3_DEFAULT_PORT],
                default => throw new \InvalidArgumentException(
                    sprintf(
                        "unsupported URL scheme '%s://' (expected 'nexus://', 'http://', 'https://', or 'resp3://')",
                        $schemeRaw,
                    ),
                ),
            };
            [$host, $port] = self::splitHostPort($rest);
            return new self($scheme, $host, $port ?? $defaultPort);
        }

        [$host, $port] = self::splitHostPort($trimmed);
        return new self('nexus', $host, $port ?? self::RPC_DEFAULT_PORT);
    }

    /**
     * @return array{0: string, 1: ?int}
     */
    private static function splitHostPort(string $s): array
    {
        if ($s === '') {
            throw new \InvalidArgumentException('missing host');
        }
        if (str_starts_with($s, '[')) {
            $end = strpos($s, ']');
            if ($end === false) {
                throw new \InvalidArgumentException(sprintf("unterminated IPv6 literal in '%s'", $s));
            }
            $host = substr($s, 1, $end - 1);
            $tail = substr($s, $end + 1);
            if ($tail === '') {
                return [$host, null];
            }
            if (!str_starts_with($tail, ':')) {
                throw new \InvalidArgumentException(sprintf("unexpected characters after IPv6 literal: '%s'", $tail));
            }
            return [$host, self::parsePort(substr($tail, 1))];
        }
        $colon = strrpos($s, ':');
        if ($colon === false) {
            return [$s, null];
        }
        $host = substr($s, 0, $colon);
        if ($host === '') {
            throw new \InvalidArgumentException(sprintf("missing host in '%s'", $s));
        }
        return [$host, self::parsePort(substr($s, $colon + 1))];
    }

    private static function parsePort(string $s): int
    {
        if (!ctype_digit($s)) {
            throw new \InvalidArgumentException(sprintf("invalid port '%s': must be 0..=65535", $s));
        }
        $n = (int) $s;
        if ($n < 0 || $n > 65535) {
            throw new \InvalidArgumentException(sprintf("invalid port '%s': must be 0..=65535", $s));
        }
        return $n;
    }
}
