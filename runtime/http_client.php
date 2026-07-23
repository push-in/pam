<?php

declare(strict_types=1);

namespace Pam\Http {
    use Pam\Async\CancellationToken;

    final readonly class ClientResponse
    {
        /** @param array<string, list<string>> $headers */
        public function __construct(
            public int $status,
            public array $headers,
            public string $body,
        ) {
        }

        public function header(string $name, ?string $default = null): ?string
        {
            $values = $this->headers[strtolower($name)] ?? [];
            return $values === [] ? $default : implode(', ', $values);
        }

        public function json(): mixed
        {
            return json_decode($this->body, true, 512, JSON_THROW_ON_ERROR);
        }

        /** @return \Generator<int, string> */
        public function serverSentEvents(): \Generator
        {
            $data = [];
            foreach (preg_split('/\R/u', $this->body) ?: [] as $line) {
                if ($line === '') {
                    if ($data !== []) {
                        yield implode("\n", $data);
                        $data = [];
                    }
                    continue;
                }
                if (str_starts_with($line, 'data:')) {
                    $data[] = ltrim(substr($line, 5));
                }
            }
            if ($data !== []) {
                yield implode("\n", $data);
            }
        }
    }

    final class Client
    {
        public function __construct(
            private readonly float $timeout = 30.0,
            private readonly int $maxResponseBytes = 16 * 1024 * 1024,
            private readonly int $maxHeaderBytes = 64 * 1024,
            private readonly int $maxRedirects = 5,
            private readonly bool $verifyTls = true,
        ) {
            if ($timeout <= 0 || $maxResponseBytes < 1 || $maxHeaderBytes < 1 || $maxRedirects < 0) {
                throw new \InvalidArgumentException('HTTP client limits must be positive.');
            }
        }

        /** @param array<string, string|list<string>> $headers */
        public function get(
            string $url,
            array $headers = [],
            ?CancellationToken $cancellation = null,
        ): ClientResponse {
            return $this->request('GET', $url, $headers, cancellation: $cancellation);
        }

        /** @param array<string, string|list<string>> $headers */
        public function post(
            string $url,
            string $body = '',
            array $headers = [],
            ?CancellationToken $cancellation = null,
        ): ClientResponse {
            return $this->request('POST', $url, $headers, $body, $cancellation);
        }

        /** @param array<string, string|list<string>> $headers */
        public function request(
            string $method,
            string $url,
            array $headers = [],
            string $body = '',
            ?CancellationToken $cancellation = null,
        ): ClientResponse {
            $method = strtoupper($method);
            if (!preg_match('/^[A-Z!#$%&\'*+.^_`|~-]+$/D', $method)) {
                throw new \InvalidArgumentException('HTTP method is invalid.');
            }
            $deadline = microtime(true) + $this->timeout;
            $redirects = 0;
            do {
                $response = $this->send($method, $url, $headers, $body, $deadline, $cancellation);
                $location = $response->header('location');
                if (
                    $location === null
                    || !in_array($response->status, [301, 302, 303, 307, 308], true)
                    || $redirects >= $this->maxRedirects
                ) {
                    return $response;
                }
                $nextUrl = self::resolveUrl($url, $location);
                if (parse_url($url, PHP_URL_HOST) !== parse_url($nextUrl, PHP_URL_HOST)) {
                    foreach (array_keys($headers) as $name) {
                        if (strtolower($name) === 'authorization') {
                            unset($headers[$name]);
                        }
                    }
                }
                if ($response->status === 303 || ($method === 'POST' && in_array($response->status, [301, 302], true))) {
                    $method = 'GET';
                    $body = '';
                }
                $url = $nextUrl;
                ++$redirects;
            } while (true);
        }

        /** @param array<string, string|list<string>> $headers */
        private function send(
            string $method,
            string $url,
            array $headers,
            string $body,
            float $deadline,
            ?CancellationToken $cancellation,
        ): ClientResponse {
            $parts = parse_url($url);
            if (!is_array($parts)) {
                throw new \InvalidArgumentException('HTTP URL is invalid.');
            }
            $scheme = strtolower(is_string($parts['scheme'] ?? null) ? $parts['scheme'] : '');
            $host = is_string($parts['host'] ?? null) ? $parts['host'] : '';
            if (
                !in_array($scheme, ['http', 'https'], true)
                || $host === ''
                || isset($parts['user'])
                || isset($parts['pass'])
            ) {
                throw new \InvalidArgumentException('Only valid http and https URLs are supported.');
            }
            $peerName = str_starts_with($host, '[') && str_ends_with($host, ']')
                ? substr($host, 1, -1)
                : $host;
            if (
                filter_var($peerName, FILTER_VALIDATE_IP) === false
                && filter_var($peerName, FILTER_VALIDATE_DOMAIN, FILTER_FLAG_HOSTNAME) === false
            ) {
                throw new \InvalidArgumentException('HTTP URL host is invalid.');
            }
            $tls = $scheme === 'https';
            $port = is_int($parts['port'] ?? null) ? $parts['port'] : ($tls ? 443 : 80);
            $target = (is_string($parts['path'] ?? null) && $parts['path'] !== '') ? $parts['path'] : '/';
            if (is_string($parts['query'] ?? null) && $parts['query'] !== '') {
                $target .= '?' . $parts['query'];
            }
            if (preg_match('/[\x00-\x20\x7f]/', $target)) {
                throw new \InvalidArgumentException('HTTP request target contains invalid control characters.');
            }
            $remaining = self::remaining($deadline);
            $stream = \Pam\Async\connect(
                "tcp://{$host}:{$port}",
                $remaining,
                $tls,
                $cancellation,
                ['ssl' => [
                    'peer_name' => $peerName,
                    'verify_peer' => $this->verifyTls,
                    'verify_peer_name' => $this->verifyTls,
                    'allow_self_signed' => !$this->verifyTls,
                    'SNI_enabled' => true,
                ]],
            );
            try {
                $normalized = self::normalizeHeaders($headers);
                $normalized['host'] ??= [$host . (($tls && $port === 443) || (!$tls && $port === 80) ? '' : ":{$port}")];
                $normalized['connection'] = ['close'];
                $normalized['accept-encoding'] ??= ['identity'];
                $normalized['user-agent'] ??= ['Pam/' . (getenv('PAM_VERSION') ?: 'dev')];
                if ($body !== '') {
                    $normalized['content-length'] = [(string) strlen($body)];
                }
                $request = "{$method} {$target} HTTP/1.1\r\n";
                foreach ($normalized as $name => $values) {
                    foreach ($values as $value) {
                        $request .= $name . ': ' . $value . "\r\n";
                    }
                }
                $request .= "\r\n" . $body;
                \Pam\Async\write($stream, $request, self::remaining($deadline), $cancellation);

                $raw = '';
                $headerEnd = false;
                while ($headerEnd === false) {
                    if (strlen($raw) > $this->maxHeaderBytes) {
                        throw new \RuntimeException('HTTP response headers exceed the configured limit.');
                    }
                    $chunk = \Pam\Async\read($stream, 8192, self::remaining($deadline), $cancellation);
                    if ($chunk === '' && feof($stream)) {
                        throw new \RuntimeException('HTTP peer closed before sending complete headers.');
                    }
                    $raw .= $chunk;
                    $headerEnd = strpos($raw, "\r\n\r\n");
                }
                $head = substr($raw, 0, $headerEnd);
                $responseBody = substr($raw, $headerEnd + 4);
                [$status, $responseHeaders] = self::parseHead($head);
                while (!feof($stream)) {
                    $chunk = \Pam\Async\read($stream, 64 * 1024, self::remaining($deadline), $cancellation);
                    if ($chunk === '' && feof($stream)) {
                        break;
                    }
                    $responseBody .= $chunk;
                    if (strlen($responseBody) > $this->maxResponseBytes + 1024) {
                        throw new \RuntimeException('HTTP response body exceeds the configured limit.');
                    }
                }
                if (str_contains(strtolower(implode(', ', $responseHeaders['transfer-encoding'] ?? [])), 'chunked')) {
                    $responseBody = self::decodeChunked($responseBody);
                } elseif (isset($responseHeaders['content-length'])) {
                    $contentLengths = array_values(array_unique($responseHeaders['content-length']));
                    if (
                        count($contentLengths) !== 1
                        || !ctype_digit($contentLengths[0])
                        || (int) $contentLengths[0] !== strlen($responseBody)
                    ) {
                        throw new \RuntimeException('HTTP response Content-Length is invalid or ambiguous.');
                    }
                }
                if (strlen($responseBody) > $this->maxResponseBytes) {
                    throw new \RuntimeException('HTTP response body exceeds the configured limit.');
                }
                return new ClientResponse($status, $responseHeaders, $responseBody);
            } finally {
                fclose($stream);
            }
        }

        /**
         * @param array<array-key, mixed> $headers
         * @return array<string, list<string>>
         */
        private static function normalizeHeaders(array $headers): array
        {
            /** @var array<string, list<string>> $result */
            $result = [];
            foreach ($headers as $name => $values) {
                if (!is_string($name)) {
                    throw new \InvalidArgumentException('HTTP header names must be strings.');
                }
                $name = strtolower($name);
                if (!preg_match('/^[a-z0-9!#$%&\'*+.^_`|~-]+$/D', $name)) {
                    throw new \InvalidArgumentException('HTTP header name is invalid.');
                }
                foreach (is_array($values) ? $values : [$values] as $value) {
                    if (!is_string($value) || str_contains($value, "\r") || str_contains($value, "\n")) {
                        throw new \InvalidArgumentException('HTTP header value is invalid.');
                    }
                    $result[$name][] = $value;
                }
            }
            return $result;
        }

        /** @return array{int, array<string, list<string>>} */
        private static function parseHead(string $head): array
        {
            $lines = explode("\r\n", $head);
            $statusLine = array_shift($lines);
            if (!preg_match('/^HTTP\/\d(?:\.\d)?\s+(\d{3})(?:\s|$)/D', $statusLine, $match)) {
                throw new \RuntimeException('HTTP response status line is invalid.');
            }
            $headers = [];
            foreach ($lines as $line) {
                if (!str_contains($line, ':')) {
                    throw new \RuntimeException('HTTP response header is invalid.');
                }
                [$name, $value] = explode(':', $line, 2);
                $name = strtolower(trim($name));
                $value = trim($value, " \t");
                if (
                    !preg_match('/^[a-z0-9!#$%&\'*+.^_`|~-]+$/D', $name)
                    || preg_match('/[\x00-\x08\x0a-\x1f\x7f]/', $value)
                ) {
                    throw new \RuntimeException('HTTP response header is invalid.');
                }
                $headers[$name][] = $value;
            }
            return [(int) $match[1], $headers];
        }

        private static function decodeChunked(string $body): string
        {
            $decoded = '';
            $offset = 0;
            while (true) {
                $lineEnd = strpos($body, "\r\n", $offset);
                if ($lineEnd === false) {
                    throw new \RuntimeException('Chunked HTTP response is truncated.');
                }
                $sizeLine = explode(';', substr($body, $offset, $lineEnd - $offset), 2)[0];
                if ($sizeLine === '' || !ctype_xdigit($sizeLine)) {
                    throw new \RuntimeException('Chunked HTTP response contains an invalid size.');
                }
                $size = hexdec($sizeLine);
                $offset = $lineEnd + 2;
                if ($size === 0) {
                    return $decoded;
                }
                if (!is_int($size) || strlen($body) < $offset + $size + 2) {
                    throw new \RuntimeException('Chunked HTTP response is truncated.');
                }
                $decoded .= substr($body, $offset, $size);
                if (substr($body, $offset + $size, 2) !== "\r\n") {
                    throw new \RuntimeException('Chunked HTTP response has an invalid delimiter.');
                }
                $offset += $size + 2;
            }
        }

        private static function remaining(float $deadline): float
        {
            $remaining = $deadline - microtime(true);
            if ($remaining <= 0) {
                throw new \RuntimeException('HTTP client request timed out.');
            }
            return $remaining;
        }

        private static function resolveUrl(string $base, string $location): string
        {
            if (parse_url($location, PHP_URL_SCHEME) !== null) {
                return $location;
            }
            $scheme = parse_url($base, PHP_URL_SCHEME);
            $host = parse_url($base, PHP_URL_HOST);
            $port = parse_url($base, PHP_URL_PORT);
            if (!is_string($scheme) || !is_string($host)) {
                throw new \RuntimeException('Cannot resolve relative redirect URL.');
            }
            if (str_starts_with($location, '//')) {
                return $scheme . ':' . $location;
            }
            $authority = $scheme . '://' . $host . (is_int($port) ? ":{$port}" : '');
            if (str_starts_with($location, '/')) {
                return $authority . $location;
            }
            $path = (string) parse_url($base, PHP_URL_PATH);
            $directory = str_contains($path, '/') ? substr($path, 0, (int) strrpos($path, '/') + 1) : '/';
            return $authority . $directory . $location;
        }
    }
}
