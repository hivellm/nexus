<?php

declare(strict_types=1);

namespace Nexus\SDK\Transport;

/**
 * Transport selector. Values match the URL-scheme tokens and the
 * NEXUS_SDK_TRANSPORT env-var strings.
 */
enum TransportMode: string
{
    case NexusRpc = 'nexus';
    case Resp3 = 'resp3';
    case Http = 'http';
    case Https = 'https';

    /**
     * Parse the NEXUS_SDK_TRANSPORT env-var token. Accepts the
     * canonical values plus the 'rpc' / 'nexusrpc' aliases. Returns
     * null for empty / 'auto' / unknown tokens.
     */
    public static function parse(?string $raw): ?self
    {
        if ($raw === null || trim($raw) === '') {
            return null;
        }
        $v = strtolower(trim($raw));
        return match ($v) {
            'nexus', 'rpc', 'nexusrpc' => self::NexusRpc,
            'resp3' => self::Resp3,
            'http' => self::Http,
            'https' => self::Https,
            'auto' => null,
            default => null,
        };
    }

    public function isRpc(): bool
    {
        return $this === self::NexusRpc;
    }
}
