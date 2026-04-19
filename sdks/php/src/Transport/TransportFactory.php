<?php

declare(strict_types=1);

namespace Nexus\SDK\Transport;

/**
 * Transport factory — applies the precedence chain and returns a
 * concrete Transport instance.
 *
 * Precedence (highest wins):
 *   1. URL scheme in $baseUrl (`nexus://` → RPC, `http://` → HTTP, …)
 *   2. NEXUS_SDK_TRANSPORT env var
 *   3. $transportHint
 *   4. Default: TransportMode::NexusRpc
 */
final class TransportFactory
{
    /**
     * @return array{transport: Transport, endpoint: Endpoint, mode: TransportMode}
     */
    public static function build(
        ?string $baseUrl,
        Credentials $credentials,
        ?TransportMode $transportHint = null,
        ?int $rpcPort = null,
        ?int $resp3Port = null,
        int $timeoutS = 30,
        ?string $envTransport = null,
    ): array {
        $endpoint = $baseUrl !== null && $baseUrl !== ''
            ? Endpoint::parse($baseUrl)
            : Endpoint::defaultLocal();

        // 1. URL scheme wins.
        $mode = self::schemeToMode($endpoint->scheme);

        // 2. Env var overrides a bare URL (no scheme).
        $explicitScheme = $baseUrl !== null && str_contains($baseUrl, '://');
        $envRaw = $envTransport ?? (getenv('NEXUS_SDK_TRANSPORT') ?: null);
        $envMode = $envRaw !== null ? TransportMode::parse($envRaw) : null;
        if ($envMode !== null && !$explicitScheme) {
            $mode = $envMode;
            $endpoint = self::realignEndpoint($endpoint, $mode, $rpcPort, $resp3Port);
        }

        // 3. Config hint.
        if ($transportHint !== null && !$explicitScheme && $envMode === null) {
            $mode = $transportHint;
            $endpoint = self::realignEndpoint($endpoint, $mode, $rpcPort, $resp3Port);
        }

        $transport = match ($mode) {
            TransportMode::NexusRpc => new RpcTransport($endpoint, $credentials),
            TransportMode::Http, TransportMode::Https => new HttpTransport($endpoint, $credentials, $timeoutS),
            TransportMode::Resp3 => throw new \InvalidArgumentException(
                "resp3 transport is not yet shipped in the PHP SDK — use 'nexus' (RPC) or 'http' for now",
            ),
        };

        return ['transport' => $transport, 'endpoint' => $endpoint, 'mode' => $mode];
    }

    private static function schemeToMode(string $scheme): TransportMode
    {
        return match ($scheme) {
            'nexus' => TransportMode::NexusRpc,
            'resp3' => TransportMode::Resp3,
            'https' => TransportMode::Https,
            default => TransportMode::Http,
        };
    }

    private static function realignEndpoint(
        Endpoint $ep,
        TransportMode $mode,
        ?int $rpcPort,
        ?int $resp3Port,
    ): Endpoint {
        return match ($mode) {
            TransportMode::NexusRpc => new Endpoint(
                'nexus',
                $ep->host,
                $rpcPort ?? Endpoint::RPC_DEFAULT_PORT,
            ),
            TransportMode::Resp3 => new Endpoint(
                'resp3',
                $ep->host,
                $resp3Port ?? Endpoint::RESP3_DEFAULT_PORT,
            ),
            TransportMode::Https => new Endpoint('https', $ep->host, Endpoint::HTTPS_DEFAULT_PORT),
            TransportMode::Http => new Endpoint('http', $ep->host, Endpoint::HTTP_DEFAULT_PORT),
        };
    }
}
