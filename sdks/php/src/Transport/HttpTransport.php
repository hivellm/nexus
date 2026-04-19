<?php

declare(strict_types=1);

namespace Nexus\SDK\Transport;

use GuzzleHttp\Client;
use GuzzleHttp\Exception\BadResponseException;
use GuzzleHttp\Exception\GuzzleException;

/**
 * HTTP fallback transport — wraps GuzzleHttp behind the Transport
 * interface. The translation from wire-level command names to HTTP
 * routes is a thin hard-coded table.
 */
final class HttpTransport implements Transport
{
    private Client $client;

    public function __construct(
        private readonly Endpoint $endpoint,
        private readonly Credentials $credentials,
        int $timeoutS = 30,
    ) {
        $headers = [
            'Accept' => 'application/json',
            'Content-Type' => 'application/json',
        ];
        if ($credentials->apiKey !== null && $credentials->apiKey !== '') {
            $headers['X-API-Key'] = $credentials->apiKey;
        } elseif (
            $credentials->username !== null && $credentials->username !== ''
            && $credentials->password !== null && $credentials->password !== ''
        ) {
            $headers['Authorization'] = 'Basic ' . base64_encode(
                $credentials->username . ':' . $credentials->password,
            );
        }
        $this->client = new Client([
            'base_uri' => $endpoint->asHttpUrl(),
            'timeout' => $timeoutS,
            'headers' => $headers,
        ]);
    }

    public function describe(): string
    {
        $tag = $this->endpoint->scheme === 'https' ? 'HTTPS' : 'HTTP';
        return (string) $this->endpoint . ' (' . $tag . ')';
    }

    public function isRpc(): bool
    {
        return false;
    }

    public function close(): void
    {
        // Guzzle has no persistent socket pool to free.
    }

    public function execute(string $command, array $args): NexusValue
    {
        return $this->dispatch($command, $args);
    }

    /**
     * @param NexusValue[] $args
     */
    private function dispatch(string $cmd, array $args): NexusValue
    {
        try {
            switch ($cmd) {
                case 'CYPHER':
                    $query = $args[0]->asString()
                        ?? throw new \InvalidArgumentException('CYPHER arg 0 must be a string');
                    $body = ['query' => $query];
                    if (count($args) > 1) {
                        $body['parameters'] = CommandMap::nexusToJson($args[1]);
                    }
                    $resp = $this->client->post('/cypher', ['json' => $body]);
                    return $this->readJson($resp);
                case 'PING':
                case 'HEALTH':
                    return $this->readJson($this->client->get('/health'));
                case 'STATS':
                    return $this->readJson($this->client->get('/stats'));
                case 'DB_LIST':
                    return $this->readJson($this->client->get('/databases'));
                case 'DB_CREATE':
                    $name = $args[0]->asString()
                        ?? throw new \InvalidArgumentException('DB_CREATE arg 0 must be a string');
                    return $this->readJson($this->client->post('/databases', [
                        'json' => ['name' => $name],
                    ]));
                case 'DB_DROP':
                    $name = $args[0]->asString()
                        ?? throw new \InvalidArgumentException('DB_DROP arg 0 must be a string');
                    return $this->readJson($this->client->delete('/databases/' . rawurlencode($name)));
                case 'DB_USE':
                    $name = $args[0]->asString()
                        ?? throw new \InvalidArgumentException('DB_USE arg 0 must be a string');
                    return $this->readJson($this->client->put('/session/database', [
                        'json' => ['name' => $name],
                    ]));
                case 'LABELS':
                    return $this->readJson($this->client->get('/schema/labels'));
                case 'REL_TYPES':
                    return $this->readJson($this->client->get('/schema/relationship-types'));
            }
            throw new \InvalidArgumentException(sprintf(
                "HTTP fallback does not know how to route '%s' — add an entry to sdks/php/src/Transport/HttpTransport.php",
                $cmd,
            ));
        } catch (BadResponseException $e) {
            $resp = $e->getResponse();
            throw new HttpRpcException($resp->getStatusCode(), (string) $resp->getBody());
        } catch (GuzzleException $e) {
            throw new \RuntimeException('HTTP request failed: ' . $e->getMessage(), 0, $e);
        }
    }

    private function readJson(\Psr\Http\Message\ResponseInterface $resp): NexusValue
    {
        if ($resp->getStatusCode() >= 400) {
            throw new HttpRpcException($resp->getStatusCode(), (string) $resp->getBody());
        }
        $text = (string) $resp->getBody();
        if ($text === '') {
            return NexusValue::null();
        }
        $decoded = json_decode($text, true);
        if (json_last_error() !== JSON_ERROR_NONE) {
            return NexusValue::str($text);
        }
        return CommandMap::jsonToNexus($decoded);
    }
}
