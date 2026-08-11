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

        public function getQuery(string $key, mixed $default = null): mixed
        {
            return $this->query[$key] ?? $default;
        }

        /** @return array<string, mixed> */
        public function query(): array
        {
            return $this->query;
        }

        public function getHeader(string $name, ?string $default = null): ?string
        {
            $values = $this->headers[strtolower($name)] ?? [];
            return $values === [] ? $default : implode(', ', $values);
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
            return new self(
                $this->method,
                $this->path,
                $this->query,
                $this->headers,
                $this->body,
                $parameters,
            );
        }
    }

    final class Response
    {
        private const STREAM_CHUNK_BYTES = 64 * 1024;

        private int $statusCode = 200;

        /** @var array<string, list<string>> */
        private array $headers = [];

        private string $body = '';

        /** @var list<string> */
        private array $chunks = [];

        private bool $streamStarted = false;

        public function status(int $statusCode): self
        {
            if ($statusCode < 100 || $statusCode > 599) {
                throw new \InvalidArgumentException('HTTP status must be between 100 and 599.');
            }

            $this->statusCode = $statusCode;
            return $this;
        }

        public function header(string $name, string $value): self
        {
            $this->headers[self::normalizeHeaderName($name)] = [self::validateHeaderValue($value)];
            return $this;
        }

        public function addHeader(string $name, string $value): self
        {
            $this->headers[self::normalizeHeaderName($name)][] = self::validateHeaderValue($value);
            return $this;
        }

        /** @param array<string, scalar|null> $options */
        public function cookie(string $name, string $value = '', array $options = []): self
        {
            $parts = [rawurlencode($name) . '=' . rawurlencode($value)];
            foreach ($options as $key => $option) {
                if ($option === false || $option === null) {
                    continue;
                }
                $label = match (strtolower($key)) {
                    'expires' => 'Expires',
                    'max-age' => 'Max-Age',
                    'domain' => 'Domain',
                    'path' => 'Path',
                    'samesite' => 'SameSite',
                    'secure' => 'Secure',
                    'httponly' => 'HttpOnly',
                    default => throw new \InvalidArgumentException("Unsupported cookie option {$key}."),
                };
                $parts[] = $option === true ? $label : $label . '=' . (string) $option;
            }
            return $this->addHeader('set-cookie', implode('; ', $parts));
        }

        public function send(string|int|float|bool|null $body): self
        {
            $this->body = match (true) {
                $body === null => '',
                is_bool($body) => $body ? 'true' : 'false',
                default => (string) $body,
            };

            if (!in_array($this->statusCode, [204, 304], true)) {
                $this->headers['content-type'] ??= ['text/plain; charset=utf-8'];
            }
            return $this;
        }

        public function json(mixed $data, int $statusCode = 200): self
        {
            $this->status($statusCode);
            $this->header('content-type', 'application/json; charset=utf-8');
            $this->body = json_encode($data, JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE);
            return $this;
        }

        /** @param iterable<string|int|float|bool|null> $chunks */
        public function stream(iterable $chunks, string $contentType = 'application/octet-stream'): self
        {
            $this->header('content-type', $contentType);
            foreach ($chunks as $chunk) {
                $normalized = match (true) {
                    $chunk === null => '',
                    is_bool($chunk) => $chunk ? 'true' : 'false',
                    default => (string) $chunk,
                };
                $this->writeChunk($normalized);
            }
            return $this;
        }

        public function writeChunk(string $chunk): self
        {
            if ($chunk === '') {
                return $this;
            }

            $length = strlen($chunk);
            for ($offset = 0; $offset < $length; $offset += self::STREAM_CHUNK_BYTES) {
                $this->emitChunk(substr($chunk, $offset, self::STREAM_CHUNK_BYTES));
            }
            return $this;
        }

        /** @param iterable<mixed> $events */
        public function sse(iterable $events): self
        {
            $this->header('cache-control', 'no-cache');
            $this->header('connection', 'keep-alive');
            $formatted = (static function () use ($events): \Generator {
                foreach ($events as $event) {
                    $payload = is_string($event)
                        ? $event
                        : json_encode($event, JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE);
                    foreach (preg_split('/\R/u', $payload) ?: [$payload] as $line) {
                        yield "data: {$line}\n";
                    }
                    yield "\n";
                }
            })();
            return $this->stream($formatted, 'text/event-stream; charset=utf-8');
        }

        public function isEmpty(): bool
        {
            return $this->body === '';
        }

        /** @return array{status: int, headers: array<string, list<string>>, body: string, chunks: list<string>} */
        public function export(): array
        {
            return [
                'status' => $this->statusCode,
                'headers' => $this->headers,
                'body' => $this->body,
                'chunks' => $this->chunks,
            ];
        }

        private function emitChunk(string $chunk): void
        {
            if (!\Pam\Async\NativeOperation::available()) {
                $this->chunks[] = $chunk;
                return;
            }

            $payload = ['chunk' => base64_encode($chunk)];
            if (!$this->streamStarted) {
                $payload['response'] = $this->export();
                $this->streamStarted = true;
            }
            \Pam\Async\NativeOperation::execute(
                \Pam\Async\OperationKind::ResponseChunk,
                $payload,
                86_400.0,
            );
        }

        private static function normalizeHeaderName(string $name): string
        {
            $name = strtolower($name);
            if ($name === '' || preg_match('/^[!#$%&\'*+.^_`|~0-9a-z-]+$/D', $name) !== 1) {
                throw new \InvalidArgumentException('HTTP header name is invalid.');
            }
            return $name;
        }

        private static function validateHeaderValue(string $value): string
        {
            if (str_contains($value, "\r") || str_contains($value, "\n") || str_contains($value, "\0")) {
                throw new \InvalidArgumentException('HTTP header value contains forbidden bytes.');
            }
            return $value;
        }
    }

    final class Server
    {
        private readonly \Closure $handler;

        public function __construct(callable $handler, private readonly bool $native = false)
        {
            $this->handler = \Closure::fromCallable($handler);
        }

        public static function create(callable $handler): self
        {
            return new self($handler, false);
        }

        /**
         * Register a runtime-aware handler that reads the request from the SAPI
         * environment and receives only the PAM response transport.
         *
         * @param callable(Response):mixed $handler
         */
        public static function createNative(callable $handler): self
        {
            return new self($handler, true);
        }

        /** @param array<string, mixed> $options */
        public function listen(
            int $port,
            string $host = '127.0.0.1',
            array $options = [],
        ): void {
            if ($this->native) {
                \Pam\Internal\Runtime::registerNativeHttpHandler($this->handler);
            } else {
                \Pam\Internal\Runtime::registerHttpHandler($this->handler);
            }
            \Pam\Internal\Runtime::listen($port, $host, $options);
        }
    }
}

namespace Pam\WS {
    enum OutboundMessageKind: int
    {
        case Event = 1;
        case Acknowledgement = 2;
        case Binary = 3;
        case Close = 4;
    }

    enum OutboundTarget: int
    {
        case Socket = 1;
        case Broadcast = 2;
        case Room = 3;
    }

    final class Socket
    {
        public function __construct(
            public readonly string $id,
            public readonly ?string $resumeToken = null,
        ) {
        }

        public function on(string $event, callable $handler): self
        {
            \Pam\Internal\Runtime::onSocketEvent($this->id, $event, $handler);
            return $this;
        }

        public function emit(string $event, mixed $data = null): self
        {
            \Pam\Internal\Runtime::emitToSocket($this->id, $event, $data);
            return $this;
        }

        public function emitBinary(string $data): self
        {
            \Pam\Internal\Runtime::emitBinaryToSocket($this->id, $data);
            return $this;
        }

        public function join(string $room): self
        {
            \Pam\Internal\Runtime::joinRoom($this->id, $room);
            return $this;
        }

        public function leave(string $room): self
        {
            \Pam\Internal\Runtime::leaveRoom($this->id, $room);
            return $this;
        }

        public function close(string $reason = 'closed by application'): void
        {
            \Pam\Internal\Runtime::closeSocket($this->id, $reason);
        }
    }

    final class Acknowledgement
    {
        private bool $sent = false;

        public function __construct(
            private readonly string $socketId,
            private readonly string|int|null $messageId,
        ) {
        }

        public function send(mixed $data = null): void
        {
            if ($this->sent || $this->messageId === null) {
                return;
            }
            $this->sent = true;
            \Pam\Internal\Runtime::acknowledge($this->socketId, $this->messageId, $data);
        }
    }

    final class RoomEmitter
    {
        public function __construct(private readonly string $room)
        {
        }

        public function emit(string $event, mixed $data = null): self
        {
            \Pam\Internal\Runtime::emitToRoom($this->room, $event, $data);
            return $this;
        }
    }

    final class Server
    {
        public function on(string $event, callable $handler): self
        {
            \Pam\Internal\Runtime::onServerEvent($event, $handler);
            return $this;
        }

        public function emit(string $event, mixed $data = null): self
        {
            \Pam\Internal\Runtime::broadcast($event, $data);
            return $this;
        }

        public function to(string $room): RoomEmitter
        {
            return new RoomEmitter($room);
        }

        public function auth(callable $authenticator): self
        {
            \Pam\Internal\Runtime::registerSocketAuthenticator($authenticator);
            return $this;
        }

        public function adapter(Adapter $adapter): self
        {
            \Pam\Internal\Runtime::registerSocketAdapter($adapter);
            return $this;
        }
    }
}

namespace Pam\Internal {
    use Pam\Http\Request;
    use Pam\Http\Response;
    use Pam\Http\Psr15\Pipeline;
    use Pam\Http\Psr7\ServerRequest as PsrServerRequest;
    use Pam\Http\Psr7\Stream as PsrStream;
    use Pam\Http\Psr7\UploadedFile as PsrUploadedFile;
    use Pam\Http\Psr7\Uri as PsrUri;
    use Pam\WS\OutboundTarget;
    use Pam\WS\OutboundMessageKind;
    use Pam\WS\Acknowledgement;
    use Pam\WS\Adapter;
    use Pam\WS\Socket;
    use Psr\Http\Message\ResponseInterface as PsrResponseInterface;
    use Psr\Http\Server\MiddlewareInterface;
    use Psr\Http\Server\RequestHandlerInterface;

    enum InboundEventKind: int
    {
        case Connection = 1;
        case Message = 2;
        case Disconnect = 3;
        case Binary = 4;
        case Tick = 5;
    }

    enum HttpDispatchState: int
    {
        case Complete = 1;
        case Suspended = 2;
    }

    enum RouteKind: int
    {
        case Legacy = 1;
        case Psr = 2;
        case WebSocket = 3;
        case Framework = 4;
        case Raw = 5;
    }

    final class HttpRequestCancelled extends \RuntimeException
    {
    }

    final class PendingHttpDispatch
    {
        /** @var array<string, mixed> */
        public array $globals = [];

        /** @var list<string> */
        public array $nativeHeaders = [];

        public int $statusCode = 200;
        public string $output = '';
        public string $sessionId = '';
        public bool $resumeSession = false;
        public ?int $outputBufferBaseLevel = null;

        /** @param \Fiber<\Throwable, mixed, string, \Pam\Async\Suspension|null> $fiber */
        public function __construct(
            public readonly string $requestId,
            public readonly \Fiber $fiber,
        ) {
        }
    }

    final class Runtime
    {
        /** @var array<string, array<string, callable>> */
        private static array $routes = [];

        /** @var array<string, mixed>|null */
        private static ?array $server = null;

        /** @var array<string, callable> */
        private static array $serverHandlers = [];

        /** @var array<string, array<string, callable>> */
        private static array $socketHandlers = [];

        /** @var array<string, true> */
        private static array $connectedSockets = [];

        /** @var array<string, array<string, true>> */
        private static array $rooms = [];

        /** @var list<array{target: int, kind: int, socketIds: list<string>, event: string, data: mixed}> */
        private static array $outbound = [];

        private static mixed $socketAuthenticator = null;

        private static ?Adapter $socketAdapter = null;

        private static string $nodeId = '';

        private static mixed $psrHandler = null;

        private static ?\Closure $httpHandler = null;

        private static ?\Closure $nativeHttpHandler = null;

        /** @var list<array{method: string, path: string, kind: int}> */
        private static array $routeMetadata = [];

        /** @var list<MiddlewareInterface|\Closure> */
        private static array $middleware = [];

        private static int $completedDispatches = 0;

        /** @var array<string, PendingHttpDispatch> */
        private static array $httpDispatches = [];

        public static function registerRoute(string $method, string $path, callable $handler): void
        {
            $method = strtoupper($method);
            if ($path === '' || $path[0] !== '/') {
                throw new \InvalidArgumentException('Route paths must start with /.');
            }

            self::$routes[$method][$path] = $handler;
        }

        public static function registerPsrHandler(mixed $handler): void
        {
            if (!interface_exists(RequestHandlerInterface::class)) {
                throw new \LogicException('Install psr/http-server-handler to use PSR-15 handlers.');
            }
            if (!$handler instanceof RequestHandlerInterface && !is_callable($handler)) {
                throw new \InvalidArgumentException('A PSR handler must be callable or implement RequestHandlerInterface.');
            }
            self::$psrHandler = $handler instanceof RequestHandlerInterface
                ? $handler
                : \Closure::fromCallable($handler);
        }

        public static function registerHttpHandler(callable $handler): void
        {
            if (self::$httpHandler !== null || self::$nativeHttpHandler !== null) {
                throw new \LogicException('An HTTP application handler is already registered.');
            }
            self::$httpHandler = \Closure::fromCallable($handler);
        }

        public static function registerNativeHttpHandler(callable $handler): void
        {
            if (self::$httpHandler !== null || self::$nativeHttpHandler !== null) {
                throw new \LogicException('An HTTP application handler is already registered.');
            }
            self::$nativeHttpHandler = \Closure::fromCallable($handler);
        }

        public static function describeRoute(string $method, string $path): void
        {
            $method = strtoupper($method);
            if ($method === '' || $path === '' || $path[0] !== '/') {
                throw new \InvalidArgumentException('Route metadata requires a method and an absolute path.');
            }
            self::$routeMetadata[] = [
                'method' => $method,
                'path' => $path,
                'kind' => RouteKind::Framework->value,
            ];
        }

        public static function registerMiddleware(mixed $middleware): void
        {
            if (!interface_exists(MiddlewareInterface::class)) {
                throw new \LogicException('Install psr/http-server-middleware to use PSR-15 middleware.');
            }
            if (!$middleware instanceof MiddlewareInterface && !is_callable($middleware)) {
                throw new \InvalidArgumentException('Middleware must be callable or implement MiddlewareInterface.');
            }
            self::$middleware[] = $middleware instanceof MiddlewareInterface
                ? $middleware
                : \Closure::fromCallable($middleware);
        }

        /** @param array<string, mixed> $options */
        public static function listen(int $port, string $host, array $options = []): void
        {
            if ($port < 1 || $port > 65535) {
                throw new \InvalidArgumentException('Port must be between 1 and 65535.');
            }
            if ($host === '') {
                throw new \InvalidArgumentException('Host cannot be empty.');
            }

            $responseStreamQueueCapacity = self::integerOption(
                $options,
                'responseStreamQueueCapacity',
                16,
            );
            $maxResponseBytes = self::integerOption(
                $options,
                'maxResponseBytes',
                256 * 1024 * 1024,
            );
            $maxResponseChunkBytes = self::integerOption(
                $options,
                'maxResponseChunkBytes',
                1024 * 1024,
            );
            if ($maxResponseChunkBytes > $maxResponseBytes) {
                throw new \InvalidArgumentException(
                    'maxResponseChunkBytes cannot exceed maxResponseBytes.',
                );
            }

            self::$server = [
                'host' => $host,
                'port' => $port,
                'maxBodyBytes' => self::integerOption($options, 'maxBodyBytes', 2 * 1024 * 1024),
                'maxHeaderBytes' => self::integerOption($options, 'maxHeaderBytes', 32 * 1024),
                'maxHeaders' => self::integerOption($options, 'maxHeaders', 100),
                'requestTimeoutMs' => self::integerOption($options, 'requestTimeoutMs', 30_000),
                'maxConcurrentRequests' => self::integerOption($options, 'maxConcurrentRequests', 4096),
                'phpExecutorQueueCapacity' => self::integerOption($options, 'phpExecutorQueueCapacity', 1_024),
                'responseStreamQueueCapacity' => $responseStreamQueueCapacity,
                'maxResponseBytes' => $maxResponseBytes,
                'maxResponseChunkBytes' => $maxResponseChunkBytes,
                'headerReadTimeoutMs' => self::integerOption($options, 'headerReadTimeoutMs', 10_000),
                'bodyReadTimeoutMs' => self::integerOption($options, 'bodyReadTimeoutMs', 30_000),
                'rateLimitPerSecond' => self::integerOption($options, 'rateLimitPerSecond', 0, true),
                'corsOrigins' => self::stringListOption($options, 'corsOrigins'),
                'trustedProxies' => self::stringListOption($options, 'trustedProxies'),
                'metricsPath' => self::stringOption($options, 'metricsPath', '/metrics'),
                'tlsCert' => self::nullableStringOption($options, 'tlsCert'),
                'tlsKey' => self::nullableStringOption($options, 'tlsKey'),
                'http3' => self::booleanOption($options, 'http3', true),
                'websocketMaxConnections' => self::integerOption($options, 'websocketMaxConnections', 10_000),
                'websocketMaxMessageBytes' => self::integerOption($options, 'websocketMaxMessageBytes', 1024 * 1024),
                'websocketQueueCapacity' => self::integerOption($options, 'websocketQueueCapacity', 256),
                'websocketHeartbeatMs' => self::integerOption($options, 'websocketHeartbeatMs', 15_000),
                'websocketTimeoutMs' => self::integerOption($options, 'websocketTimeoutMs', 45_000),
                'websocketCompression' => self::booleanOption($options, 'websocketCompression', true),
                'exposeErrors' => self::booleanOption($options, 'exposeErrors', false),
                'telemetryHeaders' => self::booleanOption($options, 'telemetryHeaders', false),
                'accessLog' => self::booleanOption($options, 'accessLog', false),
                'accessLogSampleRate' => self::integerOption($options, 'accessLogSampleRate', 1),
                'routeMetrics' => self::booleanOption($options, 'routeMetrics', false),
                'routeMetricsMaxEntries' => self::integerOption($options, 'routeMetricsMaxEntries', 256),
                'responseCachePaths' => self::absolutePathListOption($options, 'responseCachePaths'),
                'responseCacheVaryHeaders' => self::stringListOption($options, 'responseCacheVaryHeaders'),
                'responseCacheTtlMs' => self::integerOption($options, 'responseCacheTtlMs', 30_000),
                'responseCacheStaleWhileRevalidateMs' => self::integerOption($options, 'responseCacheStaleWhileRevalidateMs', 0, true),
                'responseCacheMaxEntries' => self::integerOption($options, 'responseCacheMaxEntries', 1_024),
                'responseCacheMaxBytes' => self::integerOption($options, 'responseCacheMaxBytes', 64 * 1024 * 1024),
                'responseCachePurgePath' => self::nullableStringOption($options, 'responseCachePurgePath'),
                'responseCachePurgeSecret' => self::nullableStringOption($options, 'responseCachePurgeSecret'),
                'responseCacheTagHeader' => self::stringOption($options, 'responseCacheTagHeader', 'x-pam-cache-tags'),
                'gcCollectCyclesEvery' => self::integerOption($options, 'gcCollectCyclesEvery', 256, true),
                'gcMemCachesEvery' => self::integerOption($options, 'gcMemCachesEvery', 1024, true),
                'leakDetectionSampleRate' => self::integerOption(
                    $options,
                    'leakDetectionSampleRate',
                    1024,
                    true,
                ),
                'leakThresholdBytes' => self::integerOption(
                    $options,
                    'leakThresholdBytes',
                    8 * 1024 * 1024,
                ),
                'websocketResumeSecret' => self::nullableStringOption($options, 'websocketResumeSecret'),
                'websocketResumeTtlSeconds' => self::integerOption($options, 'websocketResumeTtlSeconds', 86_400),
            ];
        }

        /** @param array<string, mixed> $options */
        private static function booleanOption(array $options, string $key, bool $default): bool
        {
            $value = $options[$key] ?? $default;
            if (!is_bool($value)) {
                throw new \InvalidArgumentException("{$key} must be a boolean.");
            }
            return $value;
        }

        /**
         * @param array<string, mixed> $options
         * @return list<string>
         */
        private static function absolutePathListOption(array $options, string $key): array
        {
            $values = self::stringListOption($options, $key);
            foreach ($values as $value) {
                if ($value === '' || $value[0] !== '/') {
                    throw new \InvalidArgumentException("{$key} entries must be absolute HTTP paths.");
                }
            }

            return array_values(array_unique($values));
        }

        /** @param array<string, mixed> $options */
        private static function integerOption(
            array $options,
            string $key,
            int $default,
            bool $allowZero = false,
        ): int {
            $value = $options[$key] ?? $default;
            if (!is_int($value) || ($allowZero ? $value < 0 : $value <= 0)) {
                throw new \InvalidArgumentException("{$key} must be a positive integer.");
            }
            return $value;
        }

        private static function serverInteger(string $key, int $default): int
        {
            $value = self::$server[$key] ?? $default;
            return is_int($value) ? $value : $default;
        }

        /**
         * @param array<string, mixed> $options
         * @return list<string>
         */
        private static function stringListOption(array $options, string $key): array
        {
            $value = $options[$key] ?? [];
            if (!is_array($value)) {
                throw new \InvalidArgumentException("{$key} must be a list of strings.");
            }
            $result = [];
            foreach ($value as $item) {
                if (!is_string($item)) {
                    throw new \InvalidArgumentException("{$key} must be a list of strings.");
                }
                $result[] = $item;
            }
            return $result;
        }

        /** @param array<string, mixed> $options */
        private static function stringOption(array $options, string $key, string $default): string
        {
            $value = $options[$key] ?? $default;
            if (!is_string($value) || $value === '') {
                throw new \InvalidArgumentException("{$key} must be a non-empty string.");
            }
            return $value;
        }

        /** @param array<string, mixed> $options */
        private static function nullableStringOption(array $options, string $key): ?string
        {
            $value = $options[$key] ?? null;
            if ($value !== null && (!is_string($value) || $value === '')) {
                throw new \InvalidArgumentException("{$key} must be a non-empty string or null.");
            }
            return $value;
        }

        public static function serverConfig(): string
        {
            return json_encode(self::$server, JSON_THROW_ON_ERROR);
        }

        public static function runtimeInfo(): string
        {
            $extensions = get_loaded_extensions();
            sort($extensions);

            return json_encode([
                'phpVersion' => PHP_VERSION,
                'phpVersionId' => PHP_VERSION_ID,
                'sapi' => PHP_SAPI,
                'zendVersion' => zend_version(),
                'nativeAbiVersion' => \Pam\Native\Api::abiVersion(),
                'zts' => (bool) PHP_ZTS,
                'debug' => (bool) PHP_DEBUG,
                'integerSize' => PHP_INT_SIZE,
                'iniLoaded' => php_ini_loaded_file() ?: null,
                'iniScanned' => array_values(array_filter(array_map(
                    static fn (string $path): string => trim($path),
                    explode(',', (string) php_ini_scanned_files()),
                ))),
                'extensions' => $extensions,
                'composerAutoloaded' => class_exists(
                    \Composer\Autoload\ClassLoader::class,
                    false,
                ),
                'xdebugLoaded' => extension_loaded('xdebug'),
                'opcacheLoaded' => extension_loaded('Zend OPcache'),
            ], JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
        }

        public static function runtimeMetrics(): string
        {
            return json_encode([
                'fibers' => \Pam\Async\Scheduler::pendingCount(),
                'memoryBytes' => memory_get_usage(true),
                'peakMemoryBytes' => memory_get_peak_usage(true),
                'httpDispatches' => count(self::$httpDispatches),
                ...\Pam\Runtime\LeakDetector::metrics(),
            ], JSON_THROW_ON_ERROR);
        }

        public static function runtimeDiagnostics(): string
        {
            $snapshot = \Pam\Diagnostics\Diagnostics::snapshot();
            $snapshot['connections'] = [
                'httpDispatches' => count(self::$httpDispatches),
                'websockets' => count(self::$connectedSockets),
                'rooms' => count(self::$rooms),
            ];
            return json_encode(
                $snapshot,
                JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES,
            );
        }

        public static function routesInfo(): string
        {
            $routes = self::$routeMetadata;
            foreach (self::$routes as $method => $methodRoutes) {
                foreach (array_keys($methodRoutes) as $path) {
                    $routes[] = [
                        'method' => $method,
                        'path' => $path,
                        'kind' => RouteKind::Legacy->value,
                    ];
                }
            }
            if (self::$psrHandler !== null) {
                $routes[] = [
                    'method' => '*',
                    'path' => '*',
                    'kind' => RouteKind::Psr->value,
                ];
            } elseif ((self::$httpHandler !== null || self::$nativeHttpHandler !== null)
                && self::$routeMetadata === []) {
                $routes[] = [
                    'method' => '*',
                    'path' => '*',
                    'kind' => RouteKind::Raw->value,
                ];
            }
            if (isset(self::$serverHandlers['connection'])) {
                $routes[] = [
                    'method' => 'WS',
                    'path' => '/ws',
                    'kind' => RouteKind::WebSocket->value,
                ];
            }
            return json_encode($routes, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
        }

        public static function beginHttpDispatch(
            string $method,
            string $target,
            string $headersJson,
            string $body,
            string $contextJson = '{}',
            string $requestId = '',
        ): string {
            if ($requestId === '') {
                throw new \InvalidArgumentException('HTTP dispatch request IDs cannot be empty.');
            }
            if (isset(self::$httpDispatches[$requestId])) {
                throw new \LogicException("HTTP dispatch {$requestId} already exists.");
            }

            /** @var \Fiber<\Throwable, mixed, string, \Pam\Async\Suspension|null> $fiber */
            $fiber = new \Fiber(static function () use (
                $method,
                $target,
                $headersJson,
                $body,
                $contextJson,
                $requestId,
            ): string {
                \Pam\Async\FiberContext::set('pam.request_id', $requestId);
                $currentFiber = \Fiber::getCurrent();
                if ($currentFiber === null) {
                    throw new \LogicException('The HTTP dispatcher must execute inside a Fiber.');
                }
                \Pam\Async\FiberContext::set('pam.native_root_fiber_id', spl_object_id($currentFiber));
                return self::executeHttp($method, $target, $headersJson, $body, $contextJson);
            });
            $dispatch = new PendingHttpDispatch($requestId, $fiber);
            self::$httpDispatches[$requestId] = $dispatch;

            return self::advanceHttpDispatch($dispatch);
        }

        public static function resumeHttpDispatch(string $requestId, string $resultJson = 'null'): string
        {
            $dispatch = self::$httpDispatches[$requestId] ?? null;
            if ($dispatch === null) {
                throw new \OutOfBoundsException("HTTP dispatch {$requestId} does not exist.");
            }
            if (!$dispatch->fiber->isSuspended()) {
                throw new \LogicException("HTTP dispatch {$requestId} is not suspended.");
            }

            \Pam\Diagnostics\Channel::publish(
                \Pam\Diagnostics\EventKind::FiberResume,
                ['requestId' => $requestId],
            );
            $result = json_decode($resultJson, true, 64, JSON_THROW_ON_ERROR);
            return self::advanceHttpDispatch($dispatch, resumeValue: $result);
        }

        public static function cancelHttpDispatch(string $requestId): void
        {
            $dispatch = self::$httpDispatches[$requestId] ?? null;
            if ($dispatch === null) {
                return;
            }

            if ($dispatch->fiber->isSuspended()) {
                self::advanceHttpDispatch(
                    $dispatch,
                    new HttpRequestCancelled("HTTP dispatch {$requestId} was cancelled."),
                );
                return;
            }

            unset(self::$httpDispatches[$requestId]);
            \Pam\Async\Scheduler::reset($requestId);
            self::clearHttpEnvironment();
            self::collectRequestMemory();
        }

        /**
         * Compatibility entry point for embedders that do not implement the
         * suspend/resume protocol. Pam's native server uses the three methods above.
         */
        public static function dispatchHttp(
            string $method,
            string $target,
            string $headersJson,
            string $body,
            string $contextJson = '{}',
        ): string {
            $context = self::decodeObject($contextJson, 32);
            $requestId = self::contextString($context, 'requestId', 'embedded-' . bin2hex(random_bytes(8)));
            $envelope = self::decodeObject(self::beginHttpDispatch(
                $method,
                $target,
                $headersJson,
                $body,
                $contextJson,
                $requestId,
            ), 32);
            $streamChunks = [];

            while (($envelope['state'] ?? null) === HttpDispatchState::Suspended->value) {
                $operation = is_array($envelope['operation'] ?? null) ? $envelope['operation'] : [];
                if (($operation['kind'] ?? null) === \Pam\Async\OperationKind::ResponseChunk->value) {
                    $chunk = is_string($operation['chunk'] ?? null)
                        ? base64_decode($operation['chunk'], true)
                        : false;
                    if (is_string($chunk)) {
                        $streamChunks[] = $chunk;
                    }
                }
                $delayMicros = is_int($operation['delayMicros'] ?? null)
                    ? max(0, $operation['delayMicros'])
                    : 0;
                if ($delayMicros > 0) {
                    usleep($delayMicros);
                }
                $envelope = self::decodeObject(self::resumeHttpDispatch($requestId), 32);
            }

            $response = $envelope['response'] ?? null;
            if (is_array($response)) {
                $response = json_encode($response, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
            }
            if (!is_string($response)) {
                throw new \RuntimeException('The HTTP dispatch completed without a response.');
            }
            if ($streamChunks !== []) {
                $decoded = self::decodeObject($response, 64);
                $existingChunks = is_array($decoded['chunks'] ?? null)
                    ? array_values(array_filter($decoded['chunks'], is_string(...)))
                    : [];
                $decoded['chunks'] = [...$existingChunks, ...$streamChunks];
                $response = json_encode($decoded, JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE);
            }
            return $response;
        }

        private static function executeHttp(
            string $method,
            string $target,
            string $headersJson,
            string $body,
            string $contextJson,
        ): string {
            $request = null;
            $response = new Response();
            $temporaryUploads = [];
            $outputLevel = ob_get_level();

            try {
                $path = parse_url($target, PHP_URL_PATH) ?: '/';
                $query = self::parseQuery((string) (parse_url($target, PHP_URL_QUERY) ?? ''));
                $headers = self::normalizeHeaders(self::decodeObject($headersJson, 512));
                $context = self::decodeObject($contextJson, 32);
                [$cookies, $parsedBody, $files, $psrUploads, $temporaryUploads] = self::requestData(
                    $headers,
                    $body,
                );
                self::populateSuperglobals(
                    strtoupper($method),
                    $target,
                    $query,
                    $headers,
                    $cookies,
                    $parsedBody,
                    $files,
                    $context,
                );
                $leakSampleRate = self::serverInteger('leakDetectionSampleRate', 1024);
                $sampleLeaks = $leakSampleRate > 0
                    && count(self::$httpDispatches) === 1
                    && (self::$completedDispatches + 1) % $leakSampleRate === 0;
                $fiberRequestId = \Pam\Async\FiberContext::get('pam.request_id', '');
                $fiberRequestId = is_string($fiberRequestId) ? $fiberRequestId : '';
                \Pam\Runtime\RequestScope::begin(
                    self::contextString($context, 'requestId', $fiberRequestId),
                    $sampleLeaks,
                );
                $profileRequestId = self::contextString(
                    $context,
                    'requestId',
                    $fiberRequestId,
                );
                \Pam\Diagnostics\Profiler::begin($profileRequestId);
                if (\Pam\Diagnostics\Channel::enabled()) {
                    \Pam\Diagnostics\Channel::publish(
                        \Pam\Diagnostics\EventKind::RequestStart,
                        ['method' => strtoupper($method), 'target' => $target],
                    );
                }
                $incomingSessionId = $cookies[session_name()] ?? null;
                if (session_status() === PHP_SESSION_NONE && is_string($incomingSessionId)) {
                    session_id($incomingSessionId);
                }
                $request = self::$nativeHttpHandler === null
                    ? new Request(strtoupper($method), $path, $query, $headers, $body)
                    : null;
                $handler = self::$routes[strtoupper($method)][$path] ?? null;

                if (self::$psrHandler !== null) {
                    $psrResponse = self::dispatchPsr(
                        strtoupper($method),
                        $target,
                        $headers,
                        $body,
                        $query,
                        $cookies,
                        $parsedBody,
                        $psrUploads,
                    );
                    $serialized = self::encodeHttpResponse(
                        self::mergeNativePsrHeaders(self::exportPsrResponse($psrResponse)),
                    );
                } elseif (self::$nativeHttpHandler !== null) {
                    $result = (self::$nativeHttpHandler)($response);
                    if ($result instanceof Response) {
                        $response = $result;
                    } elseif ($result !== null && $response->isEmpty()) {
                        $response->send($result);
                    }
                } elseif (self::$httpHandler !== null) {
                    $result = (self::$httpHandler)($request, $response);
                    if ($result instanceof Response) {
                        $response = $result;
                    } elseif ($result !== null && $response->isEmpty()) {
                        $response->send($result);
                    }
                } elseif ($handler === null) {
                    $response->json(['error' => 'Route not found'], 404);
                } else {
                    $result = $handler($request, $response);
                    if ($result instanceof Response) {
                        $response = $result;
                    } elseif ($result !== null && $response->isEmpty()) {
                        $response->send($result);
                    }
                }

                if (!isset($serialized)) {
                    self::mergeNativeHeaders($response);
                    $serialized = self::encodeHttpResponse($response->export());
                }
            } catch (HttpRequestCancelled $error) {
                $serialized = self::encodeHttpResponse([
                    'status' => 499,
                    'headers' => ['content-type' => ['application/json; charset=utf-8']],
                    'body' => json_encode(
                        ['error' => 'Request Cancelled', 'message' => $error->getMessage()],
                        JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE,
                    ),
                    'chunks' => [],
                ]);
            } catch (\Throwable $error) {
                \Pam\Observability\Telemetry::log('error', 'Unhandled request exception', [
                    'exception' => $error::class,
                    'message' => $error->getMessage(),
                    'file' => $error->getFile(),
                    'line' => $error->getLine(),
                ]);
                $exposeErrors = (bool) (self::$server['exposeErrors'] ?? false);
                $serialized = self::encodeHttpResponse([
                    'status' => 500,
                    'headers' => ['content-type' => ['application/json; charset=utf-8']],
                    'body' => json_encode(
                        [
                            'error' => 'Internal Server Error',
                            'message' => $exposeErrors
                                ? $error->getMessage()
                                : 'The request failed. Use the request ID to inspect server logs.',
                        ],
                        JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE,
                    ),
                    'chunks' => [],
                ]);
            } finally {
                if (\Pam\Diagnostics\Channel::enabled()) {
                    \Pam\Diagnostics\Channel::publish(
                        \Pam\Diagnostics\EventKind::Cleanup,
                        ['requestId' => $profileRequestId ?? null],
                    );
                }
                \Pam\Runtime\RequestScope::finish(
                    self::serverInteger('leakThresholdBytes', 8 * 1024 * 1024),
                );
                while (ob_get_level() > $outputLevel) {
                    ob_end_clean();
                }
                if (session_status() === PHP_SESSION_ACTIVE) {
                    session_write_close();
                }
                session_id('');
                // The native dispatch boundary resets the SAPI header bag and
                // response code immediately before every logical request.
                $_GET = $_POST = $_COOKIE = $_FILES = $_REQUEST = $_SERVER = [];
                $_SESSION = [];
                \Pam\Observability\Telemetry::endRequest();
                if (isset($profileRequestId)) {
                    \Pam\Diagnostics\Profiler::finish($profileRequestId);
                }
                if (\Pam\Diagnostics\Channel::enabled()) {
                    \Pam\Diagnostics\Channel::publish(
                        \Pam\Diagnostics\EventKind::RequestEnd,
                        ['requestId' => $profileRequestId ?? null],
                    );
                }
                \Pam\Async\Scheduler::reset();
                \Pam\Async\FiberContext::clear();
                foreach ($temporaryUploads as $temporaryUpload) {
                    @unlink($temporaryUpload);
                }
                unset($request, $response, $handler, $result, $psrResponse);
            }

            return $serialized;
        }

        private static function advanceHttpDispatch(
            PendingHttpDispatch $dispatch,
            ?\Throwable $interruption = null,
            mixed $resumeValue = null,
        ): string {
            if ($dispatch->globals !== []) {
                self::restoreHttpEnvironment($dispatch);
            }

            $outputLevel = $dispatch->outputBufferBaseLevel ?? ob_get_level();
            if ($dispatch->outputBufferBaseLevel === null) {
                ob_start();
            }
            $suspension = null;
            try {
                $suspension = match (true) {
                    $interruption !== null => $dispatch->fiber->throw($interruption),
                    !$dispatch->fiber->isStarted() => $dispatch->fiber->start(),
                    default => $dispatch->fiber->resume($resumeValue),
                };
                if (
                    $dispatch->fiber->isSuspended()
                    && $suspension === null
                    && class_exists(\Revolt\EventLoop::class)
                ) {
                    \Revolt\EventLoop::run();
                }
            } catch (\Throwable $error) {
                $serialized = self::internalDispatchError($error);
            } finally {
                $preserveOutputBuffers = $dispatch->fiber->isSuspended()
                    && class_exists(\Pam\Laravel\ResponseFactory::class, false)
                    && \Pam\Laravel\ResponseFactory::isStreaming();
                if ($preserveOutputBuffers) {
                    $dispatch->outputBufferBaseLevel ??= $outputLevel;
                } else {
                    while (ob_get_level() > $outputLevel + 1) {
                        $nestedOutput = ob_get_clean();
                        if ($nestedOutput !== '') {
                            echo $nestedOutput;
                        }
                    }
                    $capturedOutput = ob_get_level() > $outputLevel ? ob_get_clean() : '';
                    if ($capturedOutput !== '') {
                        $dispatch->output .= $capturedOutput;
                    }
                    $dispatch->outputBufferBaseLevel = null;
                }
            }

            if ($dispatch->fiber->isSuspended()) {
                if (!$suspension instanceof \Pam\Async\Suspension) {
                    return self::advanceHttpDispatch(
                        $dispatch,
                        new \LogicException(
                            'Only Pam async operations may suspend an HTTP request Fiber.',
                        ),
                    );
                }
                self::captureHttpEnvironment($dispatch);
                \Pam\Diagnostics\Channel::publish(
                    \Pam\Diagnostics\EventKind::FiberSuspend,
                    [
                        'requestId' => $dispatch->requestId,
                        'operationKind' => $suspension->kind->value,
                    ],
                );

                return json_encode([
                    'state' => HttpDispatchState::Suspended->value,
                    'operation' => $suspension->export(),
                ], JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES);
            }

            if (!isset($serialized)) {
                $return = $dispatch->fiber->getReturn();
                $serialized = self::mergeCapturedOutput($return, $dispatch->output);
            }

            unset(self::$httpDispatches[$dispatch->requestId]);
            self::clearHttpEnvironment();
            self::collectRequestMemory();

            return "\x89PAM\x01".$serialized;
        }

        private static function captureHttpEnvironment(PendingHttpDispatch $dispatch): void
        {
            $dispatch->resumeSession = session_status() === PHP_SESSION_ACTIVE;
            $sessionId = session_id();
            $dispatch->sessionId = is_string($sessionId) ? $sessionId : '';
            if ($dispatch->resumeSession) {
                session_write_close();
            }
            $dispatch->globals = [
                '_GET' => $_GET,
                '_POST' => $_POST,
                '_COOKIE' => $_COOKIE,
                '_FILES' => $_FILES,
                '_REQUEST' => $_REQUEST,
                '_SERVER' => $_SERVER,
                '_SESSION' => is_array($GLOBALS['_SESSION'] ?? null) ? $GLOBALS['_SESSION'] : [],
            ];
            $dispatch->nativeHeaders = headers_list();
            $statusCode = http_response_code();
            $dispatch->statusCode = is_int($statusCode) ? $statusCode : 200;
            self::clearHttpEnvironment();
        }

        private static function restoreHttpEnvironment(PendingHttpDispatch $dispatch): void
        {
            $_GET = is_array($dispatch->globals['_GET'] ?? null) ? $dispatch->globals['_GET'] : [];
            $_POST = is_array($dispatch->globals['_POST'] ?? null) ? $dispatch->globals['_POST'] : [];
            $_COOKIE = is_array($dispatch->globals['_COOKIE'] ?? null) ? $dispatch->globals['_COOKIE'] : [];
            $_FILES = is_array($dispatch->globals['_FILES'] ?? null) ? $dispatch->globals['_FILES'] : [];
            $_REQUEST = is_array($dispatch->globals['_REQUEST'] ?? null) ? $dispatch->globals['_REQUEST'] : [];
            $_SERVER = is_array($dispatch->globals['_SERVER'] ?? null) ? $dispatch->globals['_SERVER'] : [];
            $_SESSION = is_array($dispatch->globals['_SESSION'] ?? null) ? $dispatch->globals['_SESSION'] : [];

            header_remove();
            foreach ($dispatch->nativeHeaders as $header) {
                header($header, false);
            }
            http_response_code($dispatch->statusCode);

            if ($dispatch->sessionId !== '') {
                session_id($dispatch->sessionId);
            }
            if ($dispatch->resumeSession && session_status() === PHP_SESSION_NONE) {
                session_start();
            }
        }

        private static function clearHttpEnvironment(): void
        {
            if (session_status() === PHP_SESSION_ACTIVE) {
                session_write_close();
            }
            if (session_status() === PHP_SESSION_NONE && session_id() !== '') {
                session_id('');
            }
            $_GET = $_POST = $_COOKIE = $_FILES = $_REQUEST = $_SERVER = [];
            $_SESSION = [];
            header_remove();
            http_response_code(200);
        }

        private static function mergeCapturedOutput(string $serialized, string $output): string
        {
            if ($output === '') {
                return $serialized;
            }
            $response = self::decodeHttpResponse($serialized);
            if ($response['body'] !== '' || $response['chunks'] !== []) {
                return $serialized;
            }
            $response['body'] = $output;
            $headers = $response['headers'];
            $headers['content-type'] ??= ['text/plain; charset=utf-8'];
            $response['headers'] = $headers;
            return self::encodeHttpResponse($response);
        }

        /** @param array{status:int,headers:array<string,list<string>>,body:string,chunks?:list<string>} $response */
        private static function encodeHttpResponse(array $response): string
        {
            $status = $response['status'];
            $headers = $response['headers'];
            $body = $response['body'];
            $chunks = $response['chunks'] ?? [];
            if ($status < 100 || $status > 999 || count($headers) > 65535) {
                throw new \UnexpectedValueException('HTTP response metadata exceeds the native protocol limits.');
            }
            $encoded = pack('nn', $status, count($headers));
            foreach ($headers as $name => $values) {
                $name = (string) $name;
                if (strlen($name) > 65535 || count($values) > 65535) {
                    throw new \UnexpectedValueException('HTTP header exceeds the native protocol limits.');
                }
                $encoded .= pack('n', strlen($name)).$name.pack('n', count($values));
                foreach ($values as $value) {
                    $value = (string) $value;
                    $encoded .= pack('N', strlen($value)).$value;
                }
            }
            $encoded .= pack('N', strlen($body)).$body.pack('N', count($chunks));
            foreach ($chunks as $chunk) {
                $chunk = (string) $chunk;
                $encoded .= pack('N', strlen($chunk)).$chunk;
            }
            return $encoded;
        }

        /** @return array{status:int,headers:array<string,list<string>>,body:string,chunks:list<string>} */
        private static function decodeHttpResponse(string $encoded): array
        {
            $offset = 0;
            $take = static function (int $length) use ($encoded, &$offset): string {
                if ($length < 0 || $offset + $length > strlen($encoded)) {
                    throw new \UnexpectedValueException('Truncated native HTTP response.');
                }
                $value = substr($encoded, $offset, $length);
                $offset += $length;
                return $value;
            };
            $unpackInteger = static function (string $format, string $value): int {
                $unpacked = unpack($format, $value);
                if ($unpacked === false) {
                    throw new \UnexpectedValueException('Invalid native HTTP response integer.');
                }

                return (int) $unpacked['value'];
            };
            $u16 = static fn (): int => $unpackInteger('nvalue', $take(2));
            $u32 = static fn (): int => $unpackInteger('Nvalue', $take(4));
            $status = $u16();
            $headers = [];
            $headerCount = $u16();
            for ($headerIndex = 0; $headerIndex < $headerCount; ++$headerIndex) {
                $name = $take($u16());
                $values = [];
                $valueCount = $u16();
                for ($valueIndex = 0; $valueIndex < $valueCount; ++$valueIndex) {
                    $values[] = $take($u32());
                }
                $headers[$name] = $values;
            }
            $body = $take($u32());
            $chunks = [];
            $chunkCount = $u32();
            for ($chunkIndex = 0; $chunkIndex < $chunkCount; ++$chunkIndex) {
                $chunks[] = $take($u32());
            }
            return compact('status', 'headers', 'body', 'chunks');
        }

        private static function internalDispatchError(\Throwable $error): string
        {
            \Pam\Observability\Telemetry::log('error', 'HTTP dispatch protocol failure', [
                'exception' => $error::class,
                'message' => $error->getMessage(),
            ]);
            return self::encodeHttpResponse([
                'status' => 500,
                'headers' => ['content-type' => ['application/json; charset=utf-8']],
                'body' => json_encode(
                    ['error' => 'Internal Server Error', 'message' => 'The request dispatch failed.'],
                    JSON_THROW_ON_ERROR,
                ),
                'chunks' => [],
            ]);
        }

        /**
         * @param array<string, mixed> $headers
         * @return array<string, list<string>>
         */
        private static function normalizeHeaders(array $headers): array
        {
            $normalized = [];
            foreach ($headers as $name => $values) {
                $values = is_array($values) ? $values : [$values];
                $normalizedValues = [];
                foreach ($values as $value) {
                    if (!is_scalar($value) && !$value instanceof \Stringable) {
                        throw new \InvalidArgumentException('HTTP header values must be scalar.');
                    }
                    $normalizedValues[] = (string) $value;
                }
                $normalized[strtolower($name)] = $normalizedValues;
            }
            return $normalized;
        }

        /** @return array<string, mixed> */
        private static function decodeObject(string $json, int $depth): array
        {
            $decoded = json_decode($json, true, max(1, $depth), JSON_THROW_ON_ERROR);
            if (!is_array($decoded)) {
                throw new \InvalidArgumentException('Expected a JSON object.');
            }
            $result = [];
            foreach ($decoded as $key => $value) {
                if (is_string($key)) {
                    $result[$key] = $value;
                }
            }
            return $result;
        }

        /** @return array<string, mixed> */
        private static function parseQuery(string $queryString): array
        {
            $parsed = [];
            parse_str($queryString, $parsed);
            $result = [];
            foreach ($parsed as $key => $value) {
                if (is_string($key)) {
                    $result[$key] = $value;
                }
            }
            return $result;
        }

        /**
         * @param array<string, list<string>> $headers
         * @return array{
         *   array<string, string>,
         *   array<array-key, mixed>|null,
         *   array<string, array<string, int|string>>,
         *   array<string, \Psr\Http\Message\UploadedFileInterface>,
         *   list<string>
         * }
         */
        private static function requestData(array $headers, string $body): array
        {
            $cookies = [];
            foreach ($headers['cookie'] ?? [] as $line) {
                foreach (explode(';', $line) as $pair) {
                    [$name, $value] = array_pad(explode('=', trim($pair), 2), 2, '');
                    if ($name !== '') {
                        $cookies[rawurldecode($name)] = rawurldecode($value);
                    }
                }
            }

            $parsedBody = null;
            $files = [];
            $uploads = [];
            $temporaryUploads = [];
            $contentType = $headers['content-type'][0] ?? '';
            $normalizedContentType = strtolower($contentType);
            if (str_starts_with($normalizedContentType, 'application/x-www-form-urlencoded')) {
                $parsedBody = [];
                parse_str($body, $parsedBody);
            } elseif (str_starts_with($normalizedContentType, 'application/json') && $body !== '') {
                $decoded = json_decode($body, true, 512, JSON_THROW_ON_ERROR);
                $parsedBody = is_array($decoded) ? $decoded : null;
            } elseif (str_starts_with($normalizedContentType, 'multipart/form-data')) {
                [$parsedBody, $files, $uploads, $temporaryUploads] = self::parseMultipart($contentType, $body);
            }

            return [$cookies, $parsedBody, $files, $uploads, $temporaryUploads];
        }

        /** @return array{
         *   array<string, string>,
         *   array<string, array<string, int|string>>,
         *   array<string, \Psr\Http\Message\UploadedFileInterface>,
         *   list<string>
         * }
         */
        private static function parseMultipart(string $contentType, string $body): array
        {
            if (preg_match('/boundary=(?:"([^"]+)"|([^;]+))/i', $contentType, $matches) !== 1) {
                throw new \InvalidArgumentException('Multipart request is missing its boundary.');
            }
            $boundary = $matches[1] !== '' ? $matches[1] : trim($matches[2]);
            $fields = [];
            $files = [];
            $uploads = [];
            $temporaryUploads = [];

            foreach (explode('--' . $boundary, $body) as $part) {
                $part = ltrim($part, "\r\n");
                $part = preg_replace('/\r\n$/', '', $part) ?? $part;
                if ($part === '' || $part === '--') {
                    continue;
                }
                [$rawHeaders, $contents] = array_pad(explode("\r\n\r\n", $part, 2), 2, '');
                $partHeaders = [];
                foreach (explode("\r\n", $rawHeaders) as $line) {
                    if (!str_contains($line, ':')) {
                        continue;
                    }
                    [$name, $value] = explode(':', $line, 2);
                    $partHeaders[strtolower(trim($name))] = trim($value);
                }
                $disposition = $partHeaders['content-disposition'] ?? '';
                if (preg_match('/(?:^|;)\s*name="([^"]+)"/', $disposition, $nameMatch) !== 1) {
                    continue;
                }
                $name = $nameMatch[1];
                if (preg_match('/(?:^|;)\s*filename="([^"]*)"/', $disposition, $filenameMatch) !== 1) {
                    $fields[$name] = $contents;
                    continue;
                }

                $filename = basename(str_replace('\\', '/', $filenameMatch[1]));
                $mediaType = $partHeaders['content-type'] ?? 'application/octet-stream';
                $temporary = tempnam(sys_get_temp_dir(), 'pam-upload-');
                if ($temporary === false || file_put_contents($temporary, $contents) === false) {
                    throw new \RuntimeException('Unable to persist a temporary upload.');
                }
                $temporaryUploads[] = $temporary;
                $size = strlen($contents);
                $files[$name] = [
                    'name' => $filename,
                    'full_path' => $filename,
                    'type' => $mediaType,
                    'tmp_name' => $temporary,
                    'error' => UPLOAD_ERR_OK,
                    'size' => $size,
                ];
                if (class_exists(PsrUploadedFile::class)) {
                    $resource = fopen($temporary, 'rb');
                    if ($resource === false) {
                        throw new \RuntimeException('Unable to open a temporary upload.');
                    }
                    $uploads[$name] = new PsrUploadedFile(
                        new PsrStream($resource),
                        $size,
                        UPLOAD_ERR_OK,
                        $filename,
                        $mediaType,
                    );
                }
            }
            return [$fields, $files, $uploads, $temporaryUploads];
        }

        /**
         * @param array<string, mixed> $query
         * @param array<string, list<string>> $headers
         * @param array<string, string> $cookies
         * @param array<array-key, mixed>|null $parsedBody
         * @param array<string, array<string, int|string>> $files
         * @param array<string, mixed> $context
         */
        private static function populateSuperglobals(
            string $method,
            string $target,
            array $query,
            array $headers,
            array $cookies,
            ?array $parsedBody,
            array $files,
            array $context,
        ): void {
            $_GET = $query;
            $_POST = is_array($parsedBody) ? $parsedBody : [];
            $_COOKIE = $cookies;
            $_FILES = $files;
            $_REQUEST = [...$_GET, ...$_POST, ...$_COOKIE];
            $_SERVER = [
                'REQUEST_METHOD' => $method,
                'REQUEST_URI' => $target,
                'QUERY_STRING' => (string) (parse_url($target, PHP_URL_QUERY) ?? ''),
                'SERVER_PROTOCOL' => 'HTTP/' . self::contextString($context, 'protocolVersion', '1.1'),
                'REMOTE_ADDR' => self::contextString($context, 'remoteAddress', ''),
                'REMOTE_PORT' => self::contextInt($context, 'remotePort', 0),
                'REQUEST_SCHEME' => self::contextString($context, 'scheme', 'http'),
                'HTTPS' => self::contextString($context, 'scheme', 'http') === 'https' ? 'on' : 'off',
                'HTTP_HOST' => implode(', ', $headers['host'] ?? []),
                'PAM_REQUEST_ID' => self::contextString($context, 'requestId', ''),
                'PAM_TRACEPARENT' => self::contextString($context, 'traceparent', ''),
            ];
            \Pam\Observability\Telemetry::beginRequest(
                $_SERVER['PAM_REQUEST_ID'],
                $_SERVER['PAM_TRACEPARENT'],
            );
            foreach ($headers as $name => $values) {
                $serverName = match ($name) {
                    'content-type' => 'CONTENT_TYPE',
                    'content-length' => 'CONTENT_LENGTH',
                    default => 'HTTP_' . strtoupper(str_replace('-', '_', $name)),
                };
                $_SERVER[$serverName] = implode(', ', $values);
            }
        }

        /** @param array<string, mixed> $context */
        private static function contextString(array $context, string $key, string $default): string
        {
            $value = $context[$key] ?? $default;
            return is_string($value) ? $value : $default;
        }

        /** @param array<string, mixed> $context */
        private static function contextInt(array $context, string $key, int $default): int
        {
            $value = $context[$key] ?? $default;
            return is_int($value) ? $value : $default;
        }

        /**
         * @param array<string, list<string>> $headers
         * @param array<string, mixed> $query
         * @param array<string, string> $cookies
         * @param array<array-key, mixed>|null $parsedBody
         * @param array<string, \Psr\Http\Message\UploadedFileInterface> $uploads
         */
        private static function dispatchPsr(
            string $method,
            string $target,
            array $headers,
            string $body,
            array $query,
            array $cookies,
            ?array $parsedBody,
            array $uploads,
        ): PsrResponseInterface {
            if (!class_exists(PsrServerRequest::class) || !class_exists(Pipeline::class)) {
                throw new \LogicException('PSR-7 and PSR-15 Composer interfaces are required.');
            }
            $host = $headers['host'][0] ?? 'localhost';
            $scheme = $_SERVER['REQUEST_SCHEME'] ?? 'http';
            $scheme = is_string($scheme) ? $scheme : 'http';
            $uri = new PsrUri($scheme . '://' . $host . $target);
            $request = new PsrServerRequest($method, $uri, $headers, new PsrStream($body), $_SERVER);
            $request = $request
                ->withQueryParams($query)
                ->withCookieParams($cookies)
                ->withParsedBody($parsedBody)
                ->withUploadedFiles($uploads);
            $handler = self::$psrHandler;
            if (!$handler instanceof RequestHandlerInterface && !$handler instanceof \Closure) {
                throw new \LogicException('The PSR handler is invalid.');
            }
            $pipeline = new Pipeline($handler, self::$middleware);
            return $pipeline->handle($request);
        }

        /** @return array{status: int, headers: array<string, list<string>>, body: string, chunks: list<string>} */
        private static function exportPsrResponse(PsrResponseInterface $response): array
        {
            return [
                'status' => $response->getStatusCode(),
                'headers' => self::normalizeHeaders($response->getHeaders()),
                'body' => (string) $response->getBody(),
                'chunks' => [],
            ];
        }

        private static function mergeNativeHeaders(Response $response): void
        {
            $status = http_response_code();
            if (is_int($status) && $status !== 200) {
                $response->status($status);
            }
            foreach (headers_list() as $header) {
                if (!str_contains($header, ':')) {
                    continue;
                }
                [$name, $value] = explode(':', $header, 2);
                $response->addHeader(trim($name), trim($value));
            }
        }

        /**
         * @param array{status: int, headers: array<string, list<string>>, body: string, chunks: list<string>} $response
         * @return array{status: int, headers: array<string, list<string>>, body: string, chunks: list<string>}
         */
        private static function mergeNativePsrHeaders(array $response): array
        {
            $status = http_response_code();
            if (is_int($status) && $status !== 200) {
                $response['status'] = $status;
            }
            foreach (headers_list() as $header) {
                if (!str_contains($header, ':')) {
                    continue;
                }
                [$name, $value] = explode(':', $header, 2);
                $response['headers'][strtolower(trim($name))][] = trim($value);
            }
            return $response;
        }

        public static function onServerEvent(string $event, callable $handler): void
        {
            if ($event !== 'connection') {
                throw new \InvalidArgumentException('The WebSocket server supports the connection event.');
            }

            self::$serverHandlers[$event] = $handler;
        }

        public static function registerSocketAuthenticator(callable $authenticator): void
        {
            self::$socketAuthenticator = $authenticator;
        }

        public static function registerSocketAdapter(Adapter $adapter): void
        {
            self::$socketAdapter = $adapter;
            $configuredNode = getenv('PAM_NODE_ID');
            self::$nodeId = is_string($configuredNode) && $configuredNode !== ''
                ? $configuredNode
                : sprintf('%s:%s:%d', gethostname() ?: 'localhost', getenv('PAM_WORKER_ID') ?: 'standalone', getmypid());
        }

        public static function onSocketEvent(string $socketId, string $event, callable $handler): void
        {
            self::$socketHandlers[$socketId][$event] = $handler;
        }

        public static function joinRoom(string $socketId, string $room): void
        {
            if ($room === '') {
                throw new \InvalidArgumentException('Room cannot be empty.');
            }

            self::$rooms[$room][$socketId] = true;
        }

        public static function leaveRoom(string $socketId, string $room): void
        {
            unset(self::$rooms[$room][$socketId]);
            if ((self::$rooms[$room] ?? []) === []) {
                unset(self::$rooms[$room]);
            }
        }

        public static function emitToSocket(string $socketId, string $event, mixed $data): void
        {
            self::queue(OutboundTarget::Socket, OutboundMessageKind::Event, [$socketId], $event, $data);
        }

        public static function emitBinaryToSocket(string $socketId, string $data): void
        {
            self::queue(
                OutboundTarget::Socket,
                OutboundMessageKind::Binary,
                [$socketId],
                'binary',
                base64_encode($data),
            );
        }

        public static function acknowledge(string $socketId, string|int $messageId, mixed $data): void
        {
            self::queue(
                OutboundTarget::Socket,
                OutboundMessageKind::Acknowledgement,
                [$socketId],
                (string) $messageId,
                $data,
            );
        }

        public static function closeSocket(string $socketId, string $reason): void
        {
            self::queue(
                OutboundTarget::Socket,
                OutboundMessageKind::Close,
                [$socketId],
                'close',
                ['reason' => $reason],
            );
        }

        public static function broadcast(string $event, mixed $data): void
        {
            self::queue(
                OutboundTarget::Broadcast,
                OutboundMessageKind::Event,
                array_keys(self::$connectedSockets),
                $event,
                $data,
            );
            self::publishDistributed('broadcast', $event, $data);
        }

        public static function emitToRoom(string $room, string $event, mixed $data): void
        {
            self::queue(
                OutboundTarget::Room,
                OutboundMessageKind::Event,
                array_keys(self::$rooms[$room] ?? []),
                $event,
                $data,
            );
            self::publishDistributed('room:' . $room, $event, $data);
        }

        public static function dispatchWs(int $eventKind, string $socketId, string $payload): string
        {
            self::$outbound = [];

            try {
                self::drainAdapter();
                match (InboundEventKind::from($eventKind)) {
                    InboundEventKind::Connection => self::connect($socketId, $payload),
                    InboundEventKind::Message => self::message($socketId, $payload),
                    InboundEventKind::Disconnect => self::disconnect($socketId),
                    InboundEventKind::Binary => self::binary($socketId, $payload),
                    InboundEventKind::Tick => null,
                };
            } catch (\Throwable $error) {
                \Pam\Observability\Telemetry::log('error', 'Unhandled WebSocket exception', [
                    'exception' => $error::class,
                    'message' => $error->getMessage(),
                    'socketId' => $socketId,
                ]);
                self::emitToSocket($socketId, 'error', [
                    'message' => (bool) (self::$server['exposeErrors'] ?? false)
                        ? $error->getMessage()
                        : 'The WebSocket event failed.',
                ]);
            } finally {
                $serialized = json_encode(
                    self::$outbound,
                    JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE,
                );
                self::$outbound = [];
                \Pam\Async\Scheduler::reset();
                \Pam\Async\FiberContext::clear();
                self::collectRequestMemory();
            }

            return $serialized;
        }

        private static function connect(string $socketId, string $payload): void
        {
            $context = $payload === '' ? [] : self::decodeObject($payload, 64);
            $authenticator = self::$socketAuthenticator;
            if ($authenticator !== null) {
                if (!is_callable($authenticator)) {
                    throw new \LogicException('The WebSocket authenticator is invalid.');
                }
                if (!$authenticator($context)) {
                    self::closeSocket($socketId, 'authentication rejected');
                    return;
                }
            }
            self::$connectedSockets[$socketId] = true;
            self::$socketHandlers[$socketId] ??= [];

            if (isset(self::$serverHandlers['connection'])) {
                $resumeToken = $context['resumeToken'] ?? null;
                try {
                    (self::$serverHandlers['connection'])(new Socket(
                        $socketId,
                        is_string($resumeToken) ? $resumeToken : null,
                    ), $context);
                } catch (\Throwable $error) {
                    self::closeSocket($socketId, 'connection handler failed');
                    self::removeSocketState($socketId);
                    throw $error;
                }
            }
        }

        private static function message(string $socketId, string $payload): void
        {
            $message = json_decode($payload, true, 512, JSON_THROW_ON_ERROR);
            if (!is_array($message)) {
                throw new \InvalidArgumentException('WebSocket messages must be JSON objects.');
            }
            $event = $message['event'] ?? null;
            if (!is_string($event) || $event === '') {
                throw new \InvalidArgumentException('WebSocket messages require a non-empty event.');
            }

            $handler = self::$socketHandlers[$socketId][$event] ?? null;
            if ($handler === null) {
                throw new \RuntimeException("No handler registered for event {$event}.");
            }

            $messageId = $message['id'] ?? null;
            if (!is_string($messageId) && !is_int($messageId) && $messageId !== null) {
                throw new \InvalidArgumentException('WebSocket acknowledgement IDs must be strings or integers.');
            }
            $handler(
                $message['data'] ?? null,
                new Acknowledgement($socketId, $messageId),
            );
        }

        private static function binary(string $socketId, string $payload): void
        {
            $handler = self::$socketHandlers[$socketId]['binary'] ?? null;
            if ($handler === null) {
                throw new \RuntimeException('No binary WebSocket handler registered.');
            }
            $decoded = base64_decode($payload, true);
            if ($decoded === false) {
                throw new \InvalidArgumentException('Invalid binary WebSocket payload.');
            }
            $handler($decoded);
        }

        private static function disconnect(string $socketId): void
        {
            try {
                if (isset(self::$socketHandlers[$socketId]['disconnect'])) {
                    (self::$socketHandlers[$socketId]['disconnect'])();
                }
            } finally {
                self::removeSocketState($socketId);
            }
        }

        private static function removeSocketState(string $socketId): void
        {
            unset(self::$connectedSockets[$socketId], self::$socketHandlers[$socketId]);
            foreach (array_keys(self::$rooms) as $room) {
                self::leaveRoom($socketId, $room);
            }
        }

        /** @param list<string> $socketIds */
        private static function queue(
            OutboundTarget $target,
            OutboundMessageKind $kind,
            array $socketIds,
            string $event,
            mixed $data,
        ): void {
            if ($event === '') {
                throw new \InvalidArgumentException('Event cannot be empty.');
            }

            self::$outbound[] = [
                'target' => $target->value,
                'kind' => $kind->value,
                'socketIds' => $socketIds,
                'event' => $event,
                'data' => $data,
            ];
        }

        private static function publishDistributed(string $channel, string $event, mixed $data): void
        {
            if (self::$socketAdapter === null) {
                return;
            }
            self::$socketAdapter->publish($channel, json_encode([
                'node' => self::$nodeId,
                'event' => $event,
                'data' => $data,
            ], JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE));
        }

        private static function drainAdapter(): void
        {
            if (self::$socketAdapter === null) {
                return;
            }
            foreach (self::$socketAdapter->poll() as $message) {
                $payload = json_decode($message['payload'], true, 64, JSON_THROW_ON_ERROR);
                if (!is_array($payload)) {
                    continue;
                }
                if (($payload['node'] ?? null) === self::$nodeId) {
                    continue;
                }
                $channel = $message['channel'];
                if ($channel === 'broadcast') {
                    $event = $payload['event'] ?? null;
                    if (!is_string($event) || $event === '') {
                        continue;
                    }
                    self::queue(
                        OutboundTarget::Broadcast,
                        OutboundMessageKind::Event,
                        array_keys(self::$connectedSockets),
                        $event,
                        $payload['data'] ?? null,
                    );
                } elseif (str_starts_with($channel, 'room:')) {
                    $room = substr($channel, 5);
                    $event = $payload['event'] ?? null;
                    if (!is_string($event) || $event === '') {
                        continue;
                    }
                    self::queue(
                        OutboundTarget::Room,
                        OutboundMessageKind::Event,
                        array_keys(self::$rooms[$room] ?? []),
                        $event,
                        $payload['data'] ?? null,
                    );
                }
            }
        }

        private static function collectRequestMemory(): void
        {
            self::$completedDispatches++;

            $configuredCyclesEvery = self::$server['gcCollectCyclesEvery'] ?? 256;
            $cyclesEvery = is_int($configuredCyclesEvery) ? $configuredCyclesEvery : 256;
            if ($cyclesEvery > 0 && self::$completedDispatches % $cyclesEvery === 0) {
                gc_collect_cycles();
            }

            $configuredCachesEvery = self::$server['gcMemCachesEvery'] ?? 1024;
            $cachesEvery = is_int($configuredCachesEvery) ? $configuredCachesEvery : 1024;
            if ($cachesEvery > 0 && self::$completedDispatches % $cachesEvery === 0) {
                gc_mem_caches();
            }
        }
    }
}

namespace Pam {
    $arguments = $GLOBALS['argv'] ?? [];
    $script = is_array($arguments) && isset($arguments[0]) && is_string($arguments[0])
        ? $arguments[0]
        : 'pam';
    $_SERVER['PHP_SELF'] = $script;
    $_SERVER['SCRIPT_NAME'] = $_SERVER['PHP_SELF'];
    $_SERVER['SCRIPT_FILENAME'] = $_SERVER['PHP_SELF'];
    unset($arguments, $script);
}
