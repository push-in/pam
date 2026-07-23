<?php

declare(strict_types=1);

namespace Pam\Http\Psr7 {
    use Psr\Http\Message\MessageInterface;
    use Psr\Http\Message\RequestFactoryInterface;
    use Psr\Http\Message\RequestInterface;
    use Psr\Http\Message\ResponseFactoryInterface;
    use Psr\Http\Message\ResponseInterface;
    use Psr\Http\Message\ServerRequestFactoryInterface;
    use Psr\Http\Message\ServerRequestInterface;
    use Psr\Http\Message\StreamFactoryInterface;
    use Psr\Http\Message\StreamInterface;
    use Psr\Http\Message\UploadedFileFactoryInterface;
    use Psr\Http\Message\UploadedFileInterface;
    use Psr\Http\Message\UriFactoryInterface;
    use Psr\Http\Message\UriInterface;

    if (interface_exists(MessageInterface::class)) {
        final class Stream implements StreamInterface
        {
            /** @var resource|null */
            private $resource;

            /** @param resource|string $source */
            public function __construct(mixed $source = '')
            {
                if (is_resource($source)) {
                    $this->resource = $source;
                    return;
                }

                if (!is_string($source)) {
                    throw new \InvalidArgumentException('A stream requires a string or resource.');
                }

                $resource = fopen('php://temp', 'r+');
                if ($resource === false) {
                    throw new \RuntimeException('Unable to create a temporary stream.');
                }
                if ($source !== '') {
                    fwrite($resource, $source);
                    rewind($resource);
                }
                $this->resource = $resource;
            }

            public function __toString(): string
            {
                if ($this->resource === null) {
                    return '';
                }

                try {
                    $position = ftell($this->resource);
                    rewind($this->resource);
                    $contents = stream_get_contents($this->resource);
                    if ($position !== false) {
                        fseek($this->resource, $position);
                    }
                    return $contents === false ? '' : $contents;
                } catch (\Throwable) {
                    return '';
                }
            }

            public function close(): void
            {
                if ($this->resource !== null) {
                    fclose($this->resource);
                    $this->resource = null;
                }
            }

            public function detach()
            {
                $resource = $this->resource;
                $this->resource = null;
                return $resource;
            }

            public function getSize(): ?int
            {
                if ($this->resource === null) {
                    return null;
                }
                $stats = fstat($this->resource);
                return $stats === false ? null : $stats['size'];
            }

            public function tell(): int
            {
                $resource = $this->requireResource();
                $position = ftell($resource);
                if ($position === false) {
                    throw new \RuntimeException('Unable to determine stream position.');
                }
                return $position;
            }

            public function eof(): bool
            {
                return feof($this->requireResource());
            }

            public function isSeekable(): bool
            {
                return (bool) ($this->getMetadata('seekable') ?? false);
            }

            public function seek(int $offset, int $whence = SEEK_SET): void
            {
                if (!$this->isSeekable() || fseek($this->requireResource(), $offset, $whence) !== 0) {
                    throw new \RuntimeException('Unable to seek in stream.');
                }
            }

            public function rewind(): void
            {
                $this->seek(0);
            }

            public function isWritable(): bool
            {
                $mode = $this->getMetadata('mode');
                $mode = is_string($mode) ? $mode : '';
                return strpbrk($mode, 'waxc+') !== false;
            }

            public function write(string $string): int
            {
                if (!$this->isWritable()) {
                    throw new \RuntimeException('Stream is not writable.');
                }
                $written = fwrite($this->requireResource(), $string);
                if ($written === false) {
                    throw new \RuntimeException('Unable to write to stream.');
                }
                return $written;
            }

            public function isReadable(): bool
            {
                $mode = $this->getMetadata('mode');
                $mode = is_string($mode) ? $mode : '';
                return strpbrk($mode, 'r+') !== false;
            }

            public function read(int $length): string
            {
                if ($length <= 0) {
                    throw new \InvalidArgumentException('Read length must be positive.');
                }
                if (!$this->isReadable()) {
                    throw new \RuntimeException('Stream is not readable.');
                }
                $contents = fread($this->requireResource(), $length);
                if ($contents === false) {
                    throw new \RuntimeException('Unable to read from stream.');
                }
                return $contents;
            }

            public function getContents(): string
            {
                if (!$this->isReadable()) {
                    throw new \RuntimeException('Stream is not readable.');
                }
                $contents = stream_get_contents($this->requireResource());
                if ($contents === false) {
                    throw new \RuntimeException('Unable to read stream contents.');
                }
                return $contents;
            }

            public function getMetadata(?string $key = null)
            {
                if ($this->resource === null) {
                    return $key === null ? [] : null;
                }
                $metadata = stream_get_meta_data($this->resource);
                return $key === null ? $metadata : ($metadata[$key] ?? null);
            }

            /** @return resource */
            private function requireResource()
            {
                if ($this->resource === null) {
                    throw new \RuntimeException('Stream is detached.');
                }
                return $this->resource;
            }
        }

        final class Uri implements UriInterface
        {
            private string $scheme = '';
            private string $userInfo = '';
            private string $host = '';
            private ?int $port = null;
            private string $path = '';
            private string $query = '';
            private string $fragment = '';

            public function __construct(string $uri = '')
            {
                if ($uri === '') {
                    return;
                }
                $parts = parse_url($uri);
                if ($parts === false) {
                    throw new \InvalidArgumentException('Invalid URI.');
                }
                $this->scheme = strtolower((string) ($parts['scheme'] ?? ''));
                $this->host = strtolower((string) ($parts['host'] ?? ''));
                $this->port = isset($parts['port']) ? (int) $parts['port'] : null;
                $this->path = (string) ($parts['path'] ?? '');
                $this->query = (string) ($parts['query'] ?? '');
                $this->fragment = (string) ($parts['fragment'] ?? '');
                if (isset($parts['user'])) {
                    $this->userInfo = (string) $parts['user'];
                    if (isset($parts['pass'])) {
                        $this->userInfo .= ':' . $parts['pass'];
                    }
                }
            }

            public function getScheme(): string { return $this->scheme; }
            public function getUserInfo(): string { return $this->userInfo; }
            public function getHost(): string { return $this->host; }
            public function getPort(): ?int { return $this->port; }
            public function getPath(): string { return $this->path; }
            public function getQuery(): string { return $this->query; }
            public function getFragment(): string { return $this->fragment; }

            public function getAuthority(): string
            {
                if ($this->host === '') {
                    return '';
                }
                return ($this->userInfo === '' ? '' : $this->userInfo . '@')
                    . $this->host
                    . ($this->port === null ? '' : ':' . $this->port);
            }

            public function withScheme(string $scheme): UriInterface
            {
                $scheme = strtolower(rtrim($scheme, ':'));
                if ($scheme !== '' && preg_match('/^[a-z][a-z0-9+.-]*$/i', $scheme) !== 1) {
                    throw new \InvalidArgumentException('Invalid URI scheme.');
                }
                $clone = clone $this;
                $clone->scheme = $scheme;
                return $clone;
            }

            public function withUserInfo(string $user, ?string $password = null): UriInterface
            {
                $clone = clone $this;
                $clone->userInfo = $user === '' ? '' : $user . ($password === null ? '' : ':' . $password);
                return $clone;
            }

            public function withHost(string $host): UriInterface
            {
                if (preg_match('/[\s\/?#]/', $host) === 1) {
                    throw new \InvalidArgumentException('Invalid URI host.');
                }
                $clone = clone $this;
                $clone->host = strtolower($host);
                return $clone;
            }

            public function withPort(?int $port): UriInterface
            {
                if ($port !== null && ($port < 1 || $port > 65535)) {
                    throw new \InvalidArgumentException('URI port must be between 1 and 65535.');
                }
                $clone = clone $this;
                $clone->port = $port;
                return $clone;
            }

            public function withPath(string $path): UriInterface
            {
                $clone = clone $this;
                $clone->path = $path;
                return $clone;
            }

            public function withQuery(string $query): UriInterface
            {
                $clone = clone $this;
                $clone->query = ltrim($query, '?');
                return $clone;
            }

            public function withFragment(string $fragment): UriInterface
            {
                $clone = clone $this;
                $clone->fragment = ltrim($fragment, '#');
                return $clone;
            }

            public function __toString(): string
            {
                $authority = $this->getAuthority();
                $path = $this->path;
                if ($authority !== '' && ($path === '' || $path[0] !== '/')) {
                    $path = '/' . $path;
                }
                return ($this->scheme === '' ? '' : $this->scheme . ':')
                    . ($authority === '' ? '' : '//' . $authority)
                    . $path
                    . ($this->query === '' ? '' : '?' . $this->query)
                    . ($this->fragment === '' ? '' : '#' . $this->fragment);
            }
        }

        abstract class Message implements MessageInterface
        {
            protected string $protocolVersion = '1.1';

            /** @var array<string, array{name: string, values: list<string>}> */
            protected array $headers = [];

            protected StreamInterface $body;

            /** @param array<string, string|list<string>> $headers */
            public function __construct(array $headers = [], StreamInterface|string $body = '')
            {
                $this->body = is_string($body) ? new Stream($body) : $body;
                foreach ($headers as $name => $values) {
                    $this->setHeader($name, $values);
                }
            }

            public function getProtocolVersion(): string { return $this->protocolVersion; }

            public function withProtocolVersion(string $version): MessageInterface
            {
                if (preg_match('/^\d+(?:\.\d+)?$/', $version) !== 1) {
                    throw new \InvalidArgumentException('Invalid HTTP protocol version.');
                }
                $clone = clone $this;
                $clone->protocolVersion = $version;
                return $clone;
            }

            public function getHeaders(): array
            {
                $result = [];
                foreach ($this->headers as $header) {
                    $result[$header['name']] = $header['values'];
                }
                return $result;
            }

            public function hasHeader(string $name): bool { return isset($this->headers[strtolower($name)]); }

            public function getHeader(string $name): array
            {
                return $this->headers[strtolower($name)]['values'] ?? [];
            }

            public function getHeaderLine(string $name): string { return implode(', ', $this->getHeader($name)); }

            public function withHeader(string $name, $value): MessageInterface
            {
                $clone = clone $this;
                $clone->setHeader($name, $value);
                return $clone;
            }

            public function withAddedHeader(string $name, $value): MessageInterface
            {
                $clone = clone $this;
                $key = strtolower($name);
                $values = self::normalizeHeaderValues($value);
                if (isset($clone->headers[$key])) {
                    $clone->headers[$key]['values'] = [...$clone->headers[$key]['values'], ...$values];
                } else {
                    $clone->setHeader($name, $values);
                }
                return $clone;
            }

            public function withoutHeader(string $name): MessageInterface
            {
                $clone = clone $this;
                unset($clone->headers[strtolower($name)]);
                return $clone;
            }

            public function getBody(): StreamInterface { return $this->body; }

            public function withBody(StreamInterface $body): MessageInterface
            {
                $clone = clone $this;
                $clone->body = $body;
                return $clone;
            }

            protected function setHeader(string $name, mixed $value): void
            {
                if (preg_match("/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/", $name) !== 1) {
                    throw new \InvalidArgumentException('Invalid HTTP header name.');
                }
                $this->headers[strtolower($name)] = [
                    'name' => $name,
                    'values' => self::normalizeHeaderValues($value),
                ];
            }

            /** @return list<string> */
            private static function normalizeHeaderValues(mixed $value): array
            {
                $values = is_array($value) ? array_values($value) : [$value];
                $normalized = [];
                foreach ($values as $item) {
                    if (!is_string($item) && !is_numeric($item)) {
                        throw new \InvalidArgumentException('HTTP header values must be strings.');
                    }
                    $item = trim((string) $item);
                    if (preg_match('/[\r\n]/', $item) === 1) {
                        throw new \InvalidArgumentException('HTTP header values cannot contain newlines.');
                    }
                    $normalized[] = $item;
                }
                return $normalized;
            }
        }

        class ServerRequest extends Message implements ServerRequestInterface
        {
            private string $requestTarget = '';
            private string $method;
            private UriInterface $uri;
            /** @var array<string, mixed> */
            private array $serverParams;
            /** @var array<string, mixed> */
            private array $cookieParams = [];
            /** @var array<string, mixed> */
            private array $queryParams = [];
            /** @var array<array-key, mixed> */
            private array $uploadedFiles = [];
            /** @var array<array-key, mixed>|object|null */
            private object|array|null $parsedBody = null;
            /** @var array<string, mixed> */
            private array $attributes = [];

            /**
             * @param array<string, string|list<string>> $headers
             * @param array<string, mixed> $serverParams
             */
            public function __construct(
                string $method,
                UriInterface|string $uri,
                array $headers = [],
                StreamInterface|string $body = '',
                array $serverParams = [],
            ) {
                parent::__construct($headers, $body);
                if ($method === '' || preg_match('/\s/', $method) === 1) {
                    throw new \InvalidArgumentException('Invalid HTTP method.');
                }
                $this->method = $method;
                $this->uri = is_string($uri) ? new Uri($uri) : $uri;
                $this->serverParams = $serverParams;
                if (!$this->hasHeader('host') && $this->uri->getHost() !== '') {
                    $this->setHeader('Host', $this->uri->getAuthority());
                }
            }

            public function getRequestTarget(): string
            {
                if ($this->requestTarget !== '') {
                    return $this->requestTarget;
                }
                $target = $this->uri->getPath() ?: '/';
                return $target . ($this->uri->getQuery() === '' ? '' : '?' . $this->uri->getQuery());
            }

            public function withRequestTarget(string $requestTarget): RequestInterface
            {
                if ($requestTarget === '' || preg_match('/\s/', $requestTarget) === 1) {
                    throw new \InvalidArgumentException('Invalid request target.');
                }
                $clone = clone $this;
                $clone->requestTarget = $requestTarget;
                return $clone;
            }

            public function getMethod(): string { return $this->method; }

            public function withMethod(string $method): RequestInterface
            {
                if ($method === '' || preg_match('/\s/', $method) === 1) {
                    throw new \InvalidArgumentException('Invalid HTTP method.');
                }
                $clone = clone $this;
                $clone->method = $method;
                return $clone;
            }

            public function getUri(): UriInterface { return $this->uri; }

            public function withUri(UriInterface $uri, bool $preserveHost = false): RequestInterface
            {
                $clone = clone $this;
                $clone->uri = $uri;
                if (!$preserveHost || !$clone->hasHeader('host') || $clone->getHeaderLine('host') === '') {
                    if ($uri->getHost() !== '') {
                        $clone->setHeader('Host', $uri->getAuthority());
                    }
                }
                return $clone;
            }

            /** @return array<string, mixed> */
            public function getServerParams(): array { return $this->serverParams; }
            /** @return array<string, mixed> */
            public function getCookieParams(): array { return $this->cookieParams; }

            /** @param array<string, mixed> $cookies */
            public function withCookieParams(array $cookies): ServerRequestInterface
            {
                $clone = clone $this;
                $clone->cookieParams = $cookies;
                return $clone;
            }

            /** @return array<string, mixed> */
            public function getQueryParams(): array { return $this->queryParams; }

            /** @param array<string, mixed> $query */
            public function withQueryParams(array $query): ServerRequestInterface
            {
                $clone = clone $this;
                $clone->queryParams = $query;
                return $clone;
            }

            /** @return array<array-key, mixed> */
            public function getUploadedFiles(): array { return $this->uploadedFiles; }

            /** @param array<array-key, mixed> $uploadedFiles */
            public function withUploadedFiles(array $uploadedFiles): ServerRequestInterface
            {
                self::assertUploadedFiles($uploadedFiles);
                $clone = clone $this;
                $clone->uploadedFiles = $uploadedFiles;
                return $clone;
            }

            /** @return array<array-key, mixed>|object|null */
            public function getParsedBody() { return $this->parsedBody; }

            /** @param array<array-key, mixed>|object|null $data */
            public function withParsedBody($data): ServerRequestInterface
            {
                $clone = clone $this;
                $clone->parsedBody = $data;
                return $clone;
            }

            /** @return array<string, mixed> */
            public function getAttributes(): array { return $this->attributes; }
            public function getAttribute(string $name, $default = null) { return $this->attributes[$name] ?? $default; }

            public function withAttribute(string $name, $value): ServerRequestInterface
            {
                $clone = clone $this;
                $clone->attributes[$name] = $value;
                return $clone;
            }

            public function withoutAttribute(string $name): ServerRequestInterface
            {
                $clone = clone $this;
                unset($clone->attributes[$name]);
                return $clone;
            }

            /** @param array<array-key, mixed> $files */
            private static function assertUploadedFiles(array $files): void
            {
                foreach ($files as $file) {
                    if (is_array($file)) {
                        self::assertUploadedFiles($file);
                    } elseif (!$file instanceof UploadedFileInterface) {
                        throw new \InvalidArgumentException('Uploaded files must implement UploadedFileInterface.');
                    }
                }
            }
        }

        final class Request extends ServerRequest
        {
        }

        final class Response extends Message implements ResponseInterface
        {
            private int $statusCode;
            private string $reasonPhrase;

            /** @param array<string, string|list<string>> $headers */
            public function __construct(
                int $statusCode = 200,
                array $headers = [],
                StreamInterface|string $body = '',
                string $reasonPhrase = '',
            ) {
                parent::__construct($headers, $body);
                $this->assertStatus($statusCode);
                $this->statusCode = $statusCode;
                $this->reasonPhrase = $reasonPhrase;
            }

            public function getStatusCode(): int { return $this->statusCode; }

            public function withStatus(int $code, string $reasonPhrase = ''): ResponseInterface
            {
                $this->assertStatus($code);
                if (preg_match('/[\r\n]/', $reasonPhrase) === 1) {
                    throw new \InvalidArgumentException('Invalid HTTP reason phrase.');
                }
                $clone = clone $this;
                $clone->statusCode = $code;
                $clone->reasonPhrase = $reasonPhrase;
                return $clone;
            }

            public function getReasonPhrase(): string { return $this->reasonPhrase; }

            private function assertStatus(int $code): void
            {
                if ($code < 100 || $code > 599) {
                    throw new \InvalidArgumentException('HTTP status must be between 100 and 599.');
                }
            }
        }

        final class UploadedFile implements UploadedFileInterface
        {
            private bool $moved = false;

            public function __construct(
                private StreamInterface $stream,
                private readonly ?int $size = null,
                private readonly int $error = UPLOAD_ERR_OK,
                private readonly ?string $clientFilename = null,
                private readonly ?string $clientMediaType = null,
            ) {
                if (!in_array($error, [UPLOAD_ERR_OK, UPLOAD_ERR_INI_SIZE, UPLOAD_ERR_FORM_SIZE, UPLOAD_ERR_PARTIAL, UPLOAD_ERR_NO_FILE, UPLOAD_ERR_NO_TMP_DIR, UPLOAD_ERR_CANT_WRITE, UPLOAD_ERR_EXTENSION], true)) {
                    throw new \InvalidArgumentException('Invalid upload error code.');
                }
            }

            public function getStream(): StreamInterface
            {
                if ($this->moved || $this->error !== UPLOAD_ERR_OK) {
                    throw new \RuntimeException('Uploaded file stream is unavailable.');
                }
                return $this->stream;
            }

            public function moveTo(string $targetPath): void
            {
                if ($targetPath === '') {
                    throw new \InvalidArgumentException('Upload target path cannot be empty.');
                }
                if ($this->moved || $this->error !== UPLOAD_ERR_OK) {
                    throw new \RuntimeException('Uploaded file cannot be moved.');
                }
                $target = fopen($targetPath, 'wb');
                if ($target === false) {
                    throw new \RuntimeException('Unable to open upload target.');
                }
                $source = $this->stream;
                $source->rewind();
                while (!$source->eof()) {
                    if (fwrite($target, $source->read(8192)) === false) {
                        fclose($target);
                        throw new \RuntimeException('Unable to write uploaded file.');
                    }
                }
                fclose($target);
                $this->moved = true;
                $this->stream->close();
            }

            public function getSize(): ?int { return $this->size ?? $this->stream->getSize(); }
            public function getError(): int { return $this->error; }
            public function getClientFilename(): ?string { return $this->clientFilename; }
            public function getClientMediaType(): ?string { return $this->clientMediaType; }
        }

        if (
            interface_exists(RequestFactoryInterface::class)
            && interface_exists(ResponseFactoryInterface::class)
            && interface_exists(ServerRequestFactoryInterface::class)
            && interface_exists(StreamFactoryInterface::class)
            && interface_exists(UploadedFileFactoryInterface::class)
            && interface_exists(UriFactoryInterface::class)
        ) {
            final class Factory implements
                RequestFactoryInterface,
                ResponseFactoryInterface,
                ServerRequestFactoryInterface,
                StreamFactoryInterface,
                UploadedFileFactoryInterface,
                UriFactoryInterface
            {
                public function createRequest(string $method, $uri): RequestInterface
                {
                    return new Request($method, $this->uri($uri));
                }

                public function createResponse(int $code = 200, string $reasonPhrase = ''): ResponseInterface
                {
                    return new Response($code, reasonPhrase: $reasonPhrase);
                }

                /** @param array<string, mixed> $serverParams */
                public function createServerRequest(string $method, $uri, array $serverParams = []): ServerRequestInterface
                {
                    return new ServerRequest($method, $this->uri($uri), serverParams: $serverParams);
                }

                public function createStream(string $content = ''): StreamInterface { return new Stream($content); }

                public function createStreamFromFile(string $filename, string $mode = 'r'): StreamInterface
                {
                    $resource = @fopen($filename, $mode);
                    if ($resource === false) {
                        throw new \RuntimeException("Unable to open stream file {$filename}.");
                    }
                    return new Stream($resource);
                }

                public function createStreamFromResource($resource): StreamInterface
                {
                    if (!is_resource($resource)) {
                        throw new \InvalidArgumentException('Stream factory requires a resource.');
                    }
                    return new Stream($resource);
                }

                public function createUploadedFile(
                    StreamInterface $stream,
                    ?int $size = null,
                    int $error = UPLOAD_ERR_OK,
                    ?string $clientFilename = null,
                    ?string $clientMediaType = null,
                ): UploadedFileInterface {
                    return new UploadedFile($stream, $size, $error, $clientFilename, $clientMediaType);
                }

                public function createUri(string $uri = ''): UriInterface { return new Uri($uri); }

                private function uri(mixed $uri): UriInterface
                {
                    if ($uri instanceof UriInterface) {
                        return $uri;
                    }
                    if (!is_string($uri)) {
                        throw new \InvalidArgumentException('URI must be a string or UriInterface.');
                    }
                    return new Uri($uri);
                }
            }
        }
    }
}

namespace Pam\Http\Psr15 {
    use Psr\Http\Message\ResponseInterface;
    use Psr\Http\Message\ServerRequestInterface;
    use Psr\Http\Server\MiddlewareInterface;
    use Psr\Http\Server\RequestHandlerInterface;

    if (interface_exists(RequestHandlerInterface::class)) {
        final class Pipeline implements RequestHandlerInterface
        {
            /** @param list<MiddlewareInterface|callable> $middleware */
            public function __construct(
                private readonly RequestHandlerInterface|\Closure $handler,
                private readonly array $middleware = [],
                private readonly int $position = 0,
            ) {
            }

            public function handle(ServerRequestInterface $request): ResponseInterface
            {
                if (isset($this->middleware[$this->position])) {
                    $middleware = $this->middleware[$this->position];
                    $next = new self($this->handler, $this->middleware, $this->position + 1);
                    $response = $middleware instanceof MiddlewareInterface
                        ? $middleware->process($request, $next)
                        : $middleware($request, $next);
                } else {
                    $response = $this->handler instanceof RequestHandlerInterface
                        ? $this->handler->handle($request)
                        : ($this->handler)($request);
                }

                if (!$response instanceof ResponseInterface) {
                    throw new \UnexpectedValueException('A PSR-15 handler must return ResponseInterface.');
                }
                return $response;
            }
        }
    }
}
