<?php

declare(strict_types=1);

namespace Pam\Redis {
    use Pam\Async\CancellationToken;

    final class RedisException extends \RuntimeException
    {
    }

    /**
     * Fiber-cooperative RESP2 client. Socket readiness is delegated to PAM's
     * Tokio loop, so a slow Redis server does not block the embedded PHP owner.
     */
    final class Client
    {
        /** @var resource|null */
        private $stream = null;

        private string $buffer = '';

        public function __construct(
            private readonly string $host = '127.0.0.1',
            private readonly int $port = 6379,
            private readonly float $timeout = 5.0,
            private readonly ?string $password = null,
            private readonly int $database = 0,
            private readonly int $maxResponseBytes = 16 * 1024 * 1024,
            private readonly ?CancellationToken $cancellation = null,
        ) {
            if (
                $host === '' || str_contains($host, "\0") || $port < 1 || $port > 65535
                || $timeout <= 0 || $database < 0 || $maxResponseBytes < 1
            ) {
                throw new \InvalidArgumentException('Redis connection options are invalid.');
            }
        }

        public function get(string $key): ?string
        {
            $value = $this->command(['GET', $key]);
            if ($value !== null && !is_string($value)) {
                throw new RedisException('Redis GET returned an unexpected value.');
            }
            return $value;
        }

        public function set(string $key, string $value, ?int $ttlSeconds = null): bool
        {
            $command = ['SET', $key, $value];
            if ($ttlSeconds !== null) {
                if ($ttlSeconds < 1) {
                    throw new \InvalidArgumentException('Redis TTL must be positive.');
                }
                $command[] = 'EX';
                $command[] = (string) $ttlSeconds;
            }
            return $this->command($command) === 'OK';
        }

        public function delete(string ...$keys): int
        {
            if ($keys === []) {
                return 0;
            }
            $deleted = $this->command(['DEL', ...$keys]);
            if (!is_int($deleted)) {
                throw new RedisException('Redis DEL returned an unexpected value.');
            }
            return $deleted;
        }

        /** @param list<string|int|float> $arguments */
        public function command(array $arguments): mixed
        {
            if ($arguments === []) {
                throw new \InvalidArgumentException('A Redis command cannot be empty.');
            }
            $deadline = microtime(true) + $this->timeout;
            try {
                $stream = $this->connection($deadline);
                \Pam\Async\write(
                    $stream,
                    self::encode($arguments),
                    self::remaining($deadline),
                    $this->cancellation,
                );
                return $this->readValue($deadline, 0);
            } catch (\Throwable $error) {
                $this->close();
                throw $error;
            }
        }

        /**
         * @param list<list<string|int|float>> $commands
         * @return list<mixed>
         */
        public function pipeline(array $commands): array
        {
            if ($commands === []) {
                return [];
            }
            $payload = '';
            foreach ($commands as $command) {
                if ($command === []) {
                    throw new \InvalidArgumentException('Pipeline commands cannot be empty.');
                }
                $payload .= self::encode($command);
            }
            $deadline = microtime(true) + $this->timeout;
            try {
                $stream = $this->connection($deadline);
                \Pam\Async\write(
                    $stream,
                    $payload,
                    self::remaining($deadline),
                    $this->cancellation,
                );
                $responses = [];
                foreach ($commands as $_) {
                    $responses[] = $this->readValue($deadline, 0);
                }
                return $responses;
            } catch (\Throwable $error) {
                $this->close();
                throw $error;
            }
        }

        public function close(): void
        {
            if (is_resource($this->stream)) {
                fclose($this->stream);
            }
            $this->stream = null;
            $this->buffer = '';
        }

        public function __destruct()
        {
            $this->close();
        }

        /** @return resource */
        private function connection(float $deadline)
        {
            if (is_resource($this->stream) && !feof($this->stream)) {
                return $this->stream;
            }
            $this->close();
            $this->stream = \Pam\Async\connect(
                "tcp://{$this->host}:{$this->port}",
                self::remaining($deadline),
                cancellation: $this->cancellation,
            );
            if ($this->password !== null) {
                \Pam\Async\write(
                    $this->stream,
                    self::encode(['AUTH', $this->password]),
                    self::remaining($deadline),
                    $this->cancellation,
                );
                if ($this->readValue($deadline, 0) !== 'OK') {
                    throw new RedisException('Redis authentication failed.');
                }
            }
            if ($this->database !== 0) {
                \Pam\Async\write(
                    $this->stream,
                    self::encode(['SELECT', (string) $this->database]),
                    self::remaining($deadline),
                    $this->cancellation,
                );
                if ($this->readValue($deadline, 0) !== 'OK') {
                    throw new RedisException('Redis database selection failed.');
                }
            }
            return $this->stream;
        }

        private function readValue(float $deadline, int $depth): mixed
        {
            if ($depth > 32) {
                throw new RedisException('Redis response nesting exceeds 32 levels.');
            }
            $prefix = $this->readExact(1, $deadline);
            return match ($prefix) {
                '+' => $this->readLine($deadline),
                '-' => throw new RedisException($this->readLine($deadline)),
                ':' => $this->integer($this->readLine($deadline)),
                '$' => $this->readBulk($deadline),
                '*' => $this->readArray($deadline, $depth + 1),
                default => throw new RedisException('Redis returned an unknown RESP prefix.'),
            };
        }

        private function readBulk(float $deadline): ?string
        {
            $length = $this->integer($this->readLine($deadline));
            if ($length === -1) {
                return null;
            }
            if ($length < 0 || $length > $this->maxResponseBytes) {
                throw new RedisException('Redis bulk response exceeds the configured limit.');
            }
            $value = $this->readExact($length, $deadline);
            if ($this->readExact(2, $deadline) !== "\r\n") {
                throw new RedisException('Redis bulk response terminator is invalid.');
            }
            return $value;
        }

        /** @return list<mixed>|null */
        private function readArray(float $deadline, int $depth): ?array
        {
            $count = $this->integer($this->readLine($deadline));
            if ($count === -1) {
                return null;
            }
            if ($count < 0 || $count > 100_000) {
                throw new RedisException('Redis array response exceeds the configured limit.');
            }
            $values = [];
            for ($index = 0; $index < $count; ++$index) {
                $values[] = $this->readValue($deadline, $depth);
            }
            return $values;
        }

        private function readLine(float $deadline): string
        {
            while (($end = strpos($this->buffer, "\r\n")) === false) {
                $this->fill($deadline);
            }
            $line = substr($this->buffer, 0, $end);
            $this->buffer = substr($this->buffer, $end + 2);
            if (strlen($line) > $this->maxResponseBytes) {
                throw new RedisException('Redis line response exceeds the configured limit.');
            }
            return $line;
        }

        private function readExact(int $length, float $deadline): string
        {
            while (strlen($this->buffer) < $length) {
                $this->fill($deadline);
            }
            $value = substr($this->buffer, 0, $length);
            $this->buffer = substr($this->buffer, $length);
            return $value;
        }

        private function fill(float $deadline): void
        {
            if (strlen($this->buffer) > $this->maxResponseBytes) {
                throw new RedisException('Redis response buffer exceeds the configured limit.');
            }
            $chunk = \Pam\Async\read(
                $this->connection($deadline),
                64 * 1024,
                self::remaining($deadline),
                $this->cancellation,
            );
            if ($chunk === '') {
                throw new RedisException('Redis closed the connection unexpectedly.');
            }
            $this->buffer .= $chunk;
        }

        /** @param list<string|int|float> $arguments */
        private static function encode(array $arguments): string
        {
            $encoded = '*'.count($arguments)."\r\n";
            foreach ($arguments as $argument) {
                $argument = (string) $argument;
                if (strlen($argument) > 512 * 1024 * 1024) {
                    throw new \InvalidArgumentException('Redis argument is too large.');
                }
                $encoded .= '$'.strlen($argument)."\r\n{$argument}\r\n";
            }
            return $encoded;
        }

        private function integer(string $value): int
        {
            if (preg_match('/^-?(?:0|[1-9][0-9]*)$/D', $value) !== 1) {
                throw new RedisException('Redis returned an invalid integer.');
            }
            return (int) $value;
        }

        private static function remaining(float $deadline): float
        {
            $remaining = $deadline - microtime(true);
            if ($remaining <= 0) {
                throw new RedisException('Redis operation timed out.');
            }
            return $remaining;
        }
    }
}
