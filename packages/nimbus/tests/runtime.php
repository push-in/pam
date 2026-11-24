<?php

declare(strict_types=1);

namespace Pam\Contracts\Transport {
    enum TransportCapability: int
    {
        case Publish = 1;
        case Consume = 2;
        case DelayedRetry = 3;
        case DeadLetter = 4;
        case OrderedDelivery = 5;
    }

    final readonly class TransportDescriptor
    {
        /** @param list<TransportCapability> $capabilities */
        public function __construct(public string $id, private array $capabilities)
        {
        }

        public function supports(TransportCapability $capability): bool
        {
            return in_array($capability, $this->capabilities, true);
        }
    }

    interface TransportProviderInterface
    {
        public function descriptor(): TransportDescriptor;
    }

    interface TransportApplicationInterface extends \Pam\Contracts\Http\ApplicationInterface
    {
        public function transport(TransportProviderInterface $provider): self;

        /** @return array<string, TransportProviderInterface> */
        public function transports(): array;
    }
}

namespace Pam\Observability {
    final class Telemetry
    {
        /** @param array<string, mixed> $context */
        public static function log(string $level, string $message, array $context = []): void
        {
        }
    }
}

namespace Pam\Redis {
    final class Client
    {
        public function __construct(
            public readonly string $host = '127.0.0.1',
            public readonly int $port = 6379,
            public readonly float $timeout = 5.0,
            public readonly ?string $password = null,
            public readonly int $database = 0,
        ) {
        }

        /** @param list<string|int|float> $arguments */
        public function command(array $arguments): mixed
        {
            throw new \LogicException('Redis is provided by the PAM runtime.');
        }
    }
}

namespace Pam\Http {
    final readonly class ClientResponse
    {
        /** @param array<string, list<string>> $headers */
        public function __construct(
            public int $status,
            public array $headers,
            public string $body,
        ) {
        }
    }

    final class Client
    {
        public function __construct(
            public readonly float $timeout = 30.0,
            public readonly int $maxResponseBytes = 16 * 1024 * 1024,
            public readonly int $maxHeaderBytes = 64 * 1024,
            public readonly int $maxRedirects = 5,
            public readonly bool $verifyTls = true,
        ) {
        }

        /** @param array<string, string|list<string>> $headers */
        public function request(string $method, string $url, array $headers = [], string $body = ''): ClientResponse
        {
            throw new \LogicException('The HTTP client is provided by the PAM runtime.');
        }
    }
}

namespace Pam\Http {
    final class Request
    {
        /**
         * @param array<string, mixed> $query
         * @param array<string, list<string>> $headers
         * @param array<string, string> $routeParameters
         */
        public function __construct(
            public readonly string $method,
            public readonly string $path,
            private readonly array $query,
            private readonly array $headers,
            private readonly string $body,
            private readonly array $routeParameters = [],
        ) {
        }

        public function getHeader(string $name, ?string $default = null): ?string
        {
            $values = $this->headers[strtolower($name)] ?? [];
            return $values === [] ? $default : implode(', ', $values);
        }

        /** @return array<string, mixed> */
        public function query(): array
        {
            return $this->query;
        }

        /** @return array<string, list<string>> */
        public function headers(): array
        {
            return $this->headers;
        }

        public function body(): string
        {
            return $this->body;
        }

        public function json(): mixed
        {
            return json_decode($this->body, true, 512, JSON_THROW_ON_ERROR);
        }

        public function route(string $key, ?string $default = null): ?string
        {
            return $this->routeParameters[$key] ?? $default;
        }

        /** @return array<string, string> */
        public function routeParameters(): array
        {
            return $this->routeParameters;
        }

        /** @param array<string, string> $parameters */
        public function withRouteParameters(array $parameters): self
        {
            return new self($this->method, $this->path, $this->query, $this->headers, $this->body, $parameters);
        }
    }

    final class Response
    {
        private int $status = 200;

        /** @var array<string, list<string>> */
        private array $headers = [];

        private string $body = '';

        /** @var list<string> */
        private array $chunks = [];

        public function status(int $status): self
        {
            $this->status = $status;
            return $this;
        }

        public function header(string $name, string $value): self
        {
            $this->headers[strtolower($name)] = [$value];
            return $this;
        }

        public function addHeader(string $name, string $value): self
        {
            $this->headers[strtolower($name)][] = $value;
            return $this;
        }

        public function send(string|int|float|bool|null $body): self
        {
            $this->body = $body === null ? '' : (is_bool($body) ? ($body ? 'true' : 'false') : (string) $body);
            $this->headers['content-type'] ??= ['text/plain; charset=utf-8'];
            return $this;
        }

        public function json(mixed $data, int $status = 200): self
        {
            $this->status($status)->header('content-type', 'application/json; charset=utf-8');
            $this->body = json_encode($data, JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE);
            return $this;
        }

        /** @param iterable<string|int|float|bool|null> $chunks */
        public function stream(iterable $chunks, string $contentType = 'application/octet-stream'): self
        {
            $this->header('content-type', $contentType);
            foreach ($chunks as $chunk) {
                $this->chunks[] = $chunk === null ? '' : (is_bool($chunk) ? ($chunk ? 'true' : 'false') : (string) $chunk);
            }
            return $this;
        }

        public function isEmpty(): bool
        {
            return $this->body === '';
        }

        /** @return array{status: int, headers: array<string, list<string>>, body: string, chunks: list<string>} */
        public function export(): array
        {
            return ['status' => $this->status, 'headers' => $this->headers, 'body' => $this->body, 'chunks' => $this->chunks];
        }
    }

    final class Server
    {
        public static function create(callable $handler): self
        {
            return new self();
        }

        /** @param array<string, mixed> $options */
        public function listen(int $port, string $host = '127.0.0.1', array $options = []): void
        {
        }
    }
}

namespace Pam\Internal {
    final class HttpRequestCancelled extends \RuntimeException
    {
    }

    final class Runtime
    {
        public static function registerPsrHandler(object $handler): void
        {
        }

        public static function registerMiddleware(object $middleware): void
        {
        }

        /** @param array<string, mixed> $options */
        public static function listen(int $port, string $host, array $options): void
        {
        }

        public static function describeRoute(string $method, string $path): void
        {
        }
    }
}

namespace Pam\WS {
    final class RoomEmitter
    {
        public function __construct(private readonly string $room)
        {
        }

        public function emit(string $event, mixed $data = null): self
        {
            throw new \LogicException("Room {$this->room} needs the PAM WebSocket server.");
        }
    }

    final class Server
    {
        public function to(string $room): RoomEmitter
        {
            return new RoomEmitter($room);
        }
    }

    final class Socket
    {
        public function __construct(public readonly string $id)
        {
        }

        public function join(string $room): self
        {
            throw new \LogicException('Sockets need the PAM WebSocket server.');
        }
    }
}

namespace Pam\Shared {
    /**
     * In-process stand-in with the row semantics of the runtime's shared table.
     *
     * @implements \Iterator<string, array<string, int|float|string>>
     */
    class Table implements \Iterator, \Countable
    {
        public const TYPE_INT = 1;
        public const TYPE_FLOAT = 2;
        public const TYPE_STRING = 3;

        public static bool $available = false;

        /** Alias of $available kept for the tests that flip it under that name. */
        public static bool $runtime = false;

        /** @var array<string, int> */
        private array $types = [];

        /** @var array<string, array<string, int|float|string>> */
        private array $rows = [];

        /** @var list<string> */
        private array $keys = [];

        private int $position = 0;

        public function __construct(
            private readonly int $size,
            public readonly float $conflictProportion = 0.2,
            public readonly string $name = '',
            public readonly int $keySize = 64,
        ) {
        }

        public static function available(): bool
        {
            return self::$available || self::$runtime;
        }

        public function column(string $name, int $type, int $size = 0): bool
        {
            unset($size);
            $this->types[$name] = $type;
            return true;
        }

        /** @phpstan-impure */
        public function create(): bool
        {
            return true;
        }

        /** @param array<string, mixed> $values */
        public function set(string $key, array $values): bool
        {
            if (!$this->reserve($key)) {
                return false;
            }
            foreach ($values as $column => $value) {
                if (is_int($value) || is_float($value) || is_string($value)) {
                    $this->rows[$key][$column] = $value;
                }
            }
            return true;
        }

        /** @return array<string, int|float|string>|int|float|string|false */
        public function get(string $key, ?string $field = null): array|int|float|string|false
        {
            $row = $this->rows[$key] ?? null;
            if ($row === null) {
                return false;
            }
            return $field === null ? $row : ($row[$field] ?? false);
        }

        public function del(string $key): bool
        {
            $exists = isset($this->rows[$key]);
            unset($this->rows[$key]);
            return $exists;
        }

        public function incr(string $key, string $column, int|float $incrby = 1): int|float|false
        {
            if (!$this->reserve($key)) {
                return false;
            }
            $current = $this->rows[$key][$column];
            $next = (is_string($current) ? 0 : $current) + $incrby;
            return $this->rows[$key][$column] = $this->types[$column] === self::TYPE_INT ? (int) $next : (float) $next;
        }

        public function count(): int
        {
            return count($this->rows);
        }

        public function rewind(): void
        {
            $this->keys = array_keys($this->rows);
            $this->position = 0;
        }

        public function valid(): bool
        {
            return isset($this->keys[$this->position]);
        }

        /** @return array<string, int|float|string>|null */
        public function current(): ?array
        {
            return $this->rows[$this->keys[$this->position] ?? ''] ?? null;
        }

        public function key(): ?string
        {
            return $this->keys[$this->position] ?? null;
        }

        public function next(): void
        {
            ++$this->position;
        }

        private function reserve(string $key): bool
        {
            if (isset($this->rows[$key])) {
                return true;
            }
            if (count($this->rows) >= $this->size) {
                return false;
            }
            $this->rows[$key] = [];
            foreach ($this->types as $column => $type) {
                $this->rows[$key][$column] = match ($type) {
                    self::TYPE_INT => 0,
                    self::TYPE_FLOAT => 0.0,
                    default => '',
                };
            }
            return true;
        }
    }
}
