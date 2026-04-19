<?php

declare(strict_types=1);

namespace Nexus\SDK\Transport;

/**
 * Native binary RPC transport — synchronous blocking implementation.
 *
 * PHP's process model makes a background reader goroutine awkward, so
 * we keep this simple: each request writes a frame and then reads the
 * matching response synchronously. That works fine for the typical
 * request/response pattern the rest of the SDK uses.
 *
 * Concurrent requests across threads (or co-routines via ReactPHP)
 * would need a pool of connections — out of scope for §8.
 */
final class RpcTransport implements Transport
{
    private const PUSH_ID = 0xFFFFFFFF;

    /** @var resource|null */
    private $socket = null;
    private int $nextId = 1;
    private bool $closed = false;

    public function __construct(
        private readonly Endpoint $endpoint,
        private readonly Credentials $credentials,
        private readonly int $connectTimeoutMs = 5000,
    ) {
    }

    public function describe(): string
    {
        return (string) $this->endpoint . ' (RPC)';
    }

    public function isRpc(): bool
    {
        return true;
    }

    public function execute(string $command, array $args): NexusValue
    {
        $this->ensureConnected();
        $id = $this->allocId();
        $frame = Codec::encodeRequestFrame($id, $command, $args);
        if (fwrite($this->socket, $frame) === false) {
            throw new \RuntimeException('failed to send RPC frame');
        }
        $resp = $this->readResponse();
        if ($resp['id'] !== $id) {
            throw new \RuntimeException(sprintf(
                'RPC id mismatch (expected %d, got %d) — connection out of sync',
                $id,
                $resp['id'],
            ));
        }
        if (!$resp['ok']) {
            throw new \RuntimeException('server: ' . $resp['err']);
        }
        /** @var NexusValue $val */
        $val = $resp['value'];
        return $val;
    }

    public function close(): void
    {
        $this->closed = true;
        if ($this->socket !== null) {
            @fclose($this->socket);
            $this->socket = null;
        }
    }

    private function allocId(): int
    {
        $id = $this->nextId++;
        if ($id === self::PUSH_ID) {
            $id = $this->nextId++;
        }
        if ($this->nextId >= 0xFFFFFFFE) {
            $this->nextId = 1;
        }
        return $id;
    }

    private function ensureConnected(): void
    {
        if ($this->closed) {
            throw new \RuntimeException('RPC transport closed');
        }
        if ($this->socket !== null) {
            return;
        }
        $errno = 0;
        $errstr = '';
        $timeoutS = max(1, (int) ceil($this->connectTimeoutMs / 1000));
        $socket = @stream_socket_client(
            'tcp://' . $this->endpoint->authority(),
            $errno,
            $errstr,
            $timeoutS,
            STREAM_CLIENT_CONNECT,
        );
        if ($socket === false) {
            throw new \RuntimeException(sprintf(
                'failed to connect to %s: %s (errno %d)',
                $this->endpoint->authority(),
                $errstr,
                $errno,
            ));
        }
        stream_set_timeout($socket, $timeoutS);
        $this->socket = $socket;

        // HELLO 1 handshake.
        $helloFrame = Codec::encodeRequestFrame(0, 'HELLO', [NexusValue::int(1)]);
        if (fwrite($socket, $helloFrame) === false) {
            throw new \RuntimeException('failed to send HELLO');
        }
        $helloResp = $this->readResponse();
        if (!$helloResp['ok']) {
            throw new \RuntimeException('HELLO rejected by server: ' . $helloResp['err']);
        }

        // Optional AUTH.
        if ($this->credentials->hasAny()) {
            $args = $this->credentials->apiKey !== null && $this->credentials->apiKey !== ''
                ? [NexusValue::str($this->credentials->apiKey)]
                : [
                    NexusValue::str($this->credentials->username ?? ''),
                    NexusValue::str($this->credentials->password ?? ''),
                ];
            $authFrame = Codec::encodeRequestFrame(0, 'AUTH', $args);
            if (fwrite($socket, $authFrame) === false) {
                throw new \RuntimeException('failed to send AUTH');
            }
            $authResp = $this->readResponse();
            if (!$authResp['ok']) {
                throw new \RuntimeException('authentication failed: ' . $authResp['err']);
            }
        }
    }

    /**
     * @return array{id: int, ok: bool, value: ?NexusValue, err: string}
     */
    private function readResponse(): array
    {
        $header = $this->readExact(4);
        /** @var array{1: int} $unpacked */
        $unpacked = unpack('V', $header);
        $length = $unpacked[1];
        $body = $this->readExact($length);
        return Codec::decodeResponseBody($body);
    }

    private function readExact(int $n): string
    {
        if ($this->socket === null) {
            throw new \RuntimeException('RPC transport is not connected');
        }
        $buf = '';
        $remaining = $n;
        while ($remaining > 0) {
            $chunk = fread($this->socket, $remaining);
            if ($chunk === false || $chunk === '') {
                throw new \RuntimeException('RPC connection closed');
            }
            $buf .= $chunk;
            $remaining -= strlen($chunk);
        }
        return $buf;
    }
}
