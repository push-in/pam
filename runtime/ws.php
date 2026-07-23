<?php

declare(strict_types=1);

namespace Pam\WS {
    use function Pam\Async\delay;

    interface Adapter
    {
        public function publish(string $channel, string $payload): void;

        /** @return iterable<array{channel: string, payload: string}> */
        public function poll(): iterable;
    }

    final class InMemoryAdapter implements Adapter
    {
        /** @var \SplQueue<array{channel: string, payload: string}> */
        private \SplQueue $messages;

        public function __construct(private readonly int $capacity = 10_000)
        {
            if ($capacity < 1) {
                throw new \InvalidArgumentException('Adapter capacity must be positive.');
            }
            $this->messages = new \SplQueue();
        }

        public function publish(string $channel, string $payload): void
        {
            self::validateChannel($channel);
            if ($this->messages->count() >= $this->capacity) {
                $this->messages->dequeue();
            }
            $this->messages->enqueue(['channel' => $channel, 'payload' => $payload]);
        }

        public function poll(): iterable
        {
            while (!$this->messages->isEmpty()) {
                yield $this->messages->dequeue();
            }
        }

        private static function validateChannel(string $channel): void
        {
            if ($channel === '' || strlen($channel) > 512 || str_contains($channel, "\0")) {
                throw new \InvalidArgumentException('Adapter channel must contain 1 to 512 safe bytes.');
            }
        }
    }

    final class RedisStreamsAdapter implements Adapter
    {
        private ?\Redis $redis = null;
        private string $cursor = '$';
        private bool $cursorInitialized = false;

        public function __construct(
            private readonly string $host = '127.0.0.1',
            private readonly int $port = 6379,
            private readonly string $stream = 'pam:ws',
            private readonly float $timeout = 1.0,
            private readonly ?string $username = null,
            private readonly ?string $password = null,
            private readonly int $database = 0,
            private readonly bool $tls = false,
            private readonly int $maxRetries = 3,
            private readonly int $maxStreamLength = 100_000,
        ) {
            if (!class_exists(\Redis::class)) {
                throw new \LogicException('The redis PHP extension is required for RedisStreamsAdapter.');
            }
            if ($host === '' || $port < 1 || $port > 65535 || $timeout <= 0) {
                throw new \InvalidArgumentException('Redis host, port, and timeout are invalid.');
            }
            if ($stream === '' || strlen($stream) > 512 || str_contains($stream, "\0")) {
                throw new \InvalidArgumentException('Redis stream name is invalid.');
            }
            if ($database < 0 || $maxRetries < 0 || $maxStreamLength < 1) {
                throw new \InvalidArgumentException('Redis retry, database, and stream limits are invalid.');
            }
            $this->connect();
        }

        public function publish(string $channel, string $payload): void
        {
            $this->validateMessage($channel, $payload);
            $this->withRetry(function (\Redis $redis) use ($channel, $payload): void {
                $id = $redis->xAdd(
                    $this->stream,
                    '*',
                    ['channel' => $channel, 'payload' => $payload],
                    $this->maxStreamLength,
                    true,
                );
                if ($id === false) {
                    throw new \RuntimeException('Redis rejected the WebSocket event.');
                }
            });
        }

        public function poll(): iterable
        {
            $messages = $this->withRetry(fn (\Redis $redis): array|false =>
                $redis->xRead([$this->stream => $this->cursor], 100, 1)
            );
            if (!is_array($messages)) {
                return;
            }
            $streamMessages = $messages[$this->stream] ?? [];
            if (!is_array($streamMessages)) {
                return;
            }
            foreach ($streamMessages as $id => $fields) {
                if (!is_string($id)) {
                    continue;
                }
                $this->cursor = $id;
                if (!is_array($fields)) {
                    continue;
                }
                $channel = $fields['channel'] ?? null;
                $payload = $fields['payload'] ?? null;
                if (is_string($channel) && is_string($payload)) {
                    yield ['channel' => $channel, 'payload' => $payload];
                }
            }
        }

        public function healthy(): bool
        {
            try {
                return $this->withRetry(static fn (\Redis $redis): bool => $redis->ping() !== false);
            } catch (\Throwable) {
                return false;
            }
        }

        public function __destruct()
        {
            $this->disconnect();
        }

        private function connect(): \Redis
        {
            $redis = new \Redis();
            $host = $this->tls ? 'tls://' . $this->host : $this->host;
            if (!$redis->connect($host, $this->port, $this->timeout)) {
                throw new \RuntimeException('Unable to connect to Redis.');
            }
            if ($this->password !== null) {
                $credentials = $this->username === null
                    ? $this->password
                    : [$this->username, $this->password];
                if (!$redis->auth($credentials)) {
                    $redis->close();
                    throw new \RuntimeException('Redis authentication failed.');
                }
            }
            if ($this->database !== 0 && !$redis->select($this->database)) {
                $redis->close();
                throw new \RuntimeException('Unable to select the Redis database.');
            }
            $this->redis = $redis;
            if (!$this->cursorInitialized) {
                $latest = $redis->xRevRange($this->stream, '+', '-', 1);
                $id = is_array($latest) && $latest !== [] ? array_key_first($latest) : null;
                $this->cursor = is_string($id) ? $id : '0-0';
                $this->cursorInitialized = true;
            }
            return $redis;
        }

        private function disconnect(): void
        {
            if ($this->redis !== null) {
                try {
                    $this->redis->close();
                } catch (\Throwable) {
                }
                $this->redis = null;
            }
        }

        /**
         * @template T
         * @param callable(\Redis): T $operation
         * @return T
         */
        private function withRetry(callable $operation): mixed
        {
            $lastError = null;
            for ($attempt = 0; $attempt <= $this->maxRetries; ++$attempt) {
                try {
                    $redis = $this->redis ?? $this->connect();
                    return $operation($redis);
                } catch (\Throwable $error) {
                    $lastError = $error;
                    $this->disconnect();
                    if ($attempt < $this->maxRetries) {
                        delay(min(1.0, 0.05 * (2 ** $attempt)));
                    }
                }
            }
            throw new \RuntimeException('Redis operation failed after reconnect attempts.', 0, $lastError);
        }

        private function validateMessage(string $channel, string $payload): void
        {
            if ($channel === '' || strlen($channel) > 512 || str_contains($channel, "\0")) {
                throw new \InvalidArgumentException('Redis event channel is invalid.');
            }
            if (strlen($payload) > 8 * 1024 * 1024) {
                throw new \LengthException('Redis event payload exceeds 8 MiB.');
            }
        }
    }

    final class NatsAdapter implements Adapter
    {
        /** @var resource|null */
        private $socket = null;
        private string $buffer = '';

        public function __construct(
            private readonly string $host = '127.0.0.1',
            private readonly int $port = 4222,
            private readonly string $subject = 'pam.ws',
            private readonly float $timeout = 1.0,
            private readonly bool $tls = false,
            private readonly ?string $token = null,
            private readonly ?string $username = null,
            private readonly ?string $password = null,
            private readonly int $maxRetries = 3,
            private readonly int $maxMessageBytes = 8 * 1024 * 1024,
        ) {
            if ($host === '' || $port < 1 || $port > 65535 || $timeout <= 0) {
                throw new \InvalidArgumentException('NATS host, port, and timeout are invalid.');
            }
            if (preg_match('/^[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)*$/D', $subject) !== 1) {
                throw new \InvalidArgumentException('NATS subject contains unsupported characters.');
            }
            if ($maxRetries < 0 || $maxMessageBytes < 1) {
                throw new \InvalidArgumentException('NATS retry and message limits are invalid.');
            }
            $this->connect();
        }

        public function publish(string $channel, string $payload): void
        {
            if ($channel === '' || strlen($channel) > 512 || str_contains($channel, "\0")) {
                throw new \InvalidArgumentException('NATS event channel is invalid.');
            }
            $message = json_encode(['channel' => $channel, 'payload' => $payload], JSON_THROW_ON_ERROR);
            if (strlen($message) > $this->maxMessageBytes) {
                throw new \LengthException('NATS event exceeds the configured message limit.');
            }
            $this->withRetry(fn () => $this->writeFrame(
                "PUB {$this->subject} " . strlen($message) . "\r\n{$message}\r\n",
            ));
        }

        public function poll(): iterable
        {
            $messages = $this->withRetry(function (): array {
                $this->readAvailable();
                return $this->parseFrames();
            });
            foreach ($messages as $message) {
                yield $message;
            }
        }

        public function healthy(): bool
        {
            return is_resource($this->socket) && !feof($this->socket);
        }

        public function __destruct()
        {
            $this->disconnect();
        }

        private function connect(): void
        {
            $scheme = $this->tls ? 'tls' : 'tcp';
            $context = stream_context_create(['ssl' => [
                'peer_name' => $this->host,
                'verify_peer' => true,
                'verify_peer_name' => true,
            ]]);
            $socket = @stream_socket_client(
                "{$scheme}://{$this->host}:{$this->port}",
                $errorCode,
                $error,
                $this->timeout,
                STREAM_CLIENT_CONNECT,
                $context,
            );
            if ($socket === false) {
                throw new \RuntimeException("Unable to connect to NATS: {$errorCode} {$error}");
            }
            stream_set_blocking($socket, false);
            $this->socket = $socket;
            $this->buffer = '';
            $connect = ['verbose' => false, 'pedantic' => false, 'tls_required' => $this->tls];
            if ($this->token !== null) {
                $connect['auth_token'] = $this->token;
            }
            if ($this->username !== null) {
                $connect['user'] = $this->username;
            }
            if ($this->password !== null) {
                $connect['pass'] = $this->password;
            }
            $payload = json_encode($connect, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
            $this->writeFrame("CONNECT {$payload}\r\nSUB {$this->subject} 1\r\n");
        }

        private function disconnect(): void
        {
            if (is_resource($this->socket)) {
                fclose($this->socket);
            }
            $this->socket = null;
            $this->buffer = '';
        }

        /**
         * @template T
         * @param callable(): T $operation
         * @return T
         */
        private function withRetry(callable $operation): mixed
        {
            $lastError = null;
            for ($attempt = 0; $attempt <= $this->maxRetries; ++$attempt) {
                try {
                    if ($this->socket === null) {
                        $this->connect();
                    }
                    return $operation();
                } catch (\Throwable $error) {
                    $lastError = $error;
                    $this->disconnect();
                    if ($attempt < $this->maxRetries) {
                        delay(min(1.0, 0.05 * (2 ** $attempt)));
                    }
                }
            }
            throw new \RuntimeException('NATS operation failed after reconnect attempts.', 0, $lastError);
        }

        private function writeFrame(string $frame): void
        {
            if (!is_resource($this->socket)) {
                throw new \RuntimeException('NATS socket is disconnected.');
            }
            $offset = 0;
            $deadline = microtime(true) + $this->timeout;
            while ($offset < strlen($frame)) {
                $written = @fwrite($this->socket, substr($frame, $offset, 64 * 1024));
                if (is_int($written) && $written > 0) {
                    $offset += $written;
                    continue;
                }
                if (feof($this->socket) || microtime(true) >= $deadline) {
                    throw new \RuntimeException('NATS socket write failed or timed out.');
                }
                delay(0.001);
            }
        }

        private function readAvailable(): void
        {
            if (!is_resource($this->socket)) {
                throw new \RuntimeException('NATS socket is disconnected.');
            }
            $received = false;
            for ($read = 0; $read < 128; ++$read) {
                $chunk = @fread($this->socket, 8192);
                if (!is_string($chunk) || $chunk === '') {
                    break;
                }
                $received = true;
                $this->buffer .= $chunk;
                if (strlen($this->buffer) > $this->maxMessageBytes + 64 * 1024) {
                    throw new \LengthException('NATS receive buffer exceeded its configured limit.');
                }
            }
            if (!$received && feof($this->socket)) {
                throw new \RuntimeException('NATS server disconnected.');
            }
        }

        /** @return list<array{channel: string, payload: string}> */
        private function parseFrames(): array
        {
            $messages = [];
            while (($lineEnd = strpos($this->buffer, "\r\n")) !== false) {
                $line = substr($this->buffer, 0, $lineEnd);
                if ($line === 'PING') {
                    $this->buffer = substr($this->buffer, $lineEnd + 2);
                    $this->writeFrame("PONG\r\n");
                    continue;
                }
                if (!str_starts_with($line, 'MSG ')) {
                    $this->buffer = substr($this->buffer, $lineEnd + 2);
                    if (str_starts_with($line, '-ERR')) {
                        throw new \RuntimeException('NATS server rejected the connection or command.');
                    }
                    continue;
                }
                $parts = preg_split('/\s+/', $line);
                $length = is_array($parts) ? (int) end($parts) : -1;
                if ($length < 0 || $length > $this->maxMessageBytes) {
                    throw new \LengthException('NATS message has an invalid payload length.');
                }
                $payloadStart = $lineEnd + 2;
                $frameLength = $payloadStart + $length + 2;
                if (strlen($this->buffer) < $frameLength) {
                    break;
                }
                $payload = substr($this->buffer, $payloadStart, $length);
                if (substr($this->buffer, $payloadStart + $length, 2) !== "\r\n") {
                    throw new \RuntimeException('NATS message framing is invalid.');
                }
                $this->buffer = substr($this->buffer, $frameLength);
                $decoded = json_decode($payload, true, 16, JSON_THROW_ON_ERROR);
                $channel = is_array($decoded) ? ($decoded['channel'] ?? null) : null;
                $messagePayload = is_array($decoded) ? ($decoded['payload'] ?? null) : null;
                if (is_string($channel) && is_string($messagePayload)) {
                    $messages[] = ['channel' => $channel, 'payload' => $messagePayload];
                }
            }
            return $messages;
        }
    }
}
