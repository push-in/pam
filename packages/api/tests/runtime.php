<?php

declare(strict_types=1);

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

        public function isEmpty(): bool
        {
            return $this->body === '';
        }

        /** @return array{status: int, headers: array<string, list<string>>, body: string, chunks: list<string>} */
        public function export(): array
        {
            return ['status' => $this->status, 'headers' => $this->headers, 'body' => $this->body, 'chunks' => []];
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
