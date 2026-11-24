<?php

declare(strict_types=1);

namespace Nimbus\Http\Client;

/**
 * An immutable, fluent request builder. Each `with*()` returns a copy, so a configured
 * builder (base URL, token) can be kept in a service and shared by concurrent requests.
 *
 * ```php
 * $user = Http::withToken($token)->timeout(5)->retry(3, 200)
 *     ->get('https://api.example.com/users/1')->throw()->json();
 * ```
 */
final class PendingRequest
{
    private string $baseUrl = '';

    /** @var array<string, string> */
    private array $headers = ['accept' => 'application/json'];

    /** @var array<string, mixed> */
    private array $query = [];

    private string $format = 'json';

    private ?string $rawBody = null;

    private float $timeout = 30.0;

    private bool $verifyTls = true;

    private int $maxRedirects = 5;

    private int $tries = 1;

    /** @var \Closure(int, Response|ConnectionException): int Milliseconds before the next attempt. */
    private \Closure $backoff;

    /** @var (\Closure(Response|ConnectionException, OutgoingRequest): bool)|null */
    private ?\Closure $retryWhen = null;

    private bool $throwAfterRetries = true;

    private bool $throwOnFailure = false;

    public function __construct(private readonly Transport $transport)
    {
        $this->backoff = static fn (): int => 0;
    }

    public function baseUrl(string $url): self
    {
        return $this->with('baseUrl', rtrim($url, '/'));
    }

    /** @param array<string, string> $headers */
    public function withHeaders(array $headers): self
    {
        $copy = clone $this;
        foreach ($headers as $name => $value) {
            $copy->headers[strtolower($name)] = $value;
        }
        return $copy;
    }

    public function withHeader(string $name, string $value): self
    {
        return $this->withHeaders([$name => $value]);
    }

    public function withToken(#[\SensitiveParameter] string $token, string $type = 'Bearer'): self
    {
        return $this->withHeaders(['authorization' => trim($type . ' ' . $token)]);
    }

    public function withBasicAuth(string $username, #[\SensitiveParameter] string $password): self
    {
        return $this->withHeaders(['authorization' => 'Basic ' . base64_encode($username . ':' . $password)]);
    }

    public function withUserAgent(string $userAgent): self
    {
        return $this->withHeaders(['user-agent' => $userAgent]);
    }

    public function accept(string $contentType): self
    {
        return $this->withHeaders(['accept' => $contentType]);
    }

    public function acceptJson(): self
    {
        return $this->accept('application/json');
    }

    /** @param array<string, mixed> $query Merged into the query string of every request. */
    public function withQueryParameters(array $query): self
    {
        $copy = clone $this;
        $copy->query = $query + $copy->query;
        return $copy;
    }

    /** Sends array data as a JSON body (the default). */
    public function asJson(): self
    {
        return $this->with('format', 'json');
    }

    /** Sends array data as `application/x-www-form-urlencoded`. */
    public function asForm(): self
    {
        return $this->with('format', 'form');
    }

    /** Sends this raw body with the content type, ignoring the data of the call. */
    public function withBody(string $body, string $contentType = 'application/json'): self
    {
        $copy = $this->withHeaders(['content-type' => $contentType]);
        $copy->rawBody = $body;
        return $copy;
    }

    /** Total seconds for each attempt, including redirects. */
    public function timeout(float $seconds): self
    {
        if ($seconds <= 0 || $seconds > 3600) {
            throw new \InvalidArgumentException('HTTP timeout must be between 0 and 3600 seconds.');
        }
        return $this->with('timeout', $seconds);
    }

    public function withoutVerifying(): self
    {
        return $this->with('verifyTls', false);
    }

    public function withoutRedirecting(): self
    {
        return $this->with('maxRedirects', 0);
    }

    public function maxRedirects(int $redirects): self
    {
        if ($redirects < 0) {
            throw new \InvalidArgumentException('Maximum redirects cannot be negative.');
        }
        return $this->with('maxRedirects', $redirects);
    }

    /**
     * Retries connection failures, 5xx and 429 responses (or what `$when` accepts) up to
     * `$times` attempts in total. When they are exhausted a failed response throws a
     * RequestException unless `$throw` is false. Waits suspend only the request Fiber in PAM.
     *
     * @param int|list<int>|(\Closure(int, Response|ConnectionException): int) $sleepMilliseconds
     *        Fixed wait, a wait per retry, or a function of the attempt number and outcome.
     * @param (\Closure(Response|ConnectionException, OutgoingRequest): bool)|null $when
     */
    public function retry(int $times, int|array|\Closure $sleepMilliseconds = 0, ?\Closure $when = null, bool $throw = true): self
    {
        if ($times < 1 || $times > 100) {
            throw new \InvalidArgumentException('HTTP retries must be between 1 and 100 attempts.');
        }
        $copy = clone $this;
        $copy->tries = $times;
        $copy->backoff = match (true) {
            $sleepMilliseconds instanceof \Closure => $sleepMilliseconds,
            is_array($sleepMilliseconds) => static fn (int $attempt): int
                => $sleepMilliseconds[min($attempt - 1, count($sleepMilliseconds) - 1)] ?? 0,
            default => static fn (): int => $sleepMilliseconds,
        };
        $copy->retryWhen = $when;
        $copy->throwAfterRetries = $throw;
        return $copy;
    }

    /** Every 4xx/5xx response of this builder throws a RequestException. */
    public function throw(): self
    {
        return $this->with('throwOnFailure', true);
    }

    /** @param array<string, mixed> $query */
    public function get(string $url, array $query = []): Response
    {
        return $this->send('GET', $url, query: $query);
    }

    /** @param array<string, mixed> $query */
    public function head(string $url, array $query = []): Response
    {
        return $this->send('HEAD', $url, query: $query);
    }

    /** @param array<array-key, mixed> $data */
    public function post(string $url, array $data = []): Response
    {
        return $this->send('POST', $url, $data);
    }

    /** @param array<array-key, mixed> $data */
    public function put(string $url, array $data = []): Response
    {
        return $this->send('PUT', $url, $data);
    }

    /** @param array<array-key, mixed> $data */
    public function patch(string $url, array $data = []): Response
    {
        return $this->send('PATCH', $url, $data);
    }

    /** @param array<array-key, mixed> $data */
    public function delete(string $url, array $data = []): Response
    {
        return $this->send('DELETE', $url, $data);
    }

    /**
     * @param array<array-key, mixed> $data Body of methods other than GET and HEAD.
     * @param array<string, mixed> $query
     * @throws ConnectionException
     * @throws RequestException
     */
    public function send(string $method, string $url, array $data = [], array $query = []): Response
    {
        $request = $this->build($method, $url, $data, $query);
        for ($attempt = 1; ; ++$attempt) {
            try {
                $outcome = $this->transport->send($request);
            } catch (ConnectionException $failure) {
                $outcome = $failure;
            }
            if ($attempt >= $this->tries || !$this->shouldRetry($outcome, $request)) {
                return $this->finish($outcome, $this->tries > 1);
            }
            self::sleep(($this->backoff)($attempt, $outcome));
        }
    }

    /**
     * Sends the pool's requests concurrently: in PAM each runs in its own Fiber, with
     * symfony/http-client they are multiplexed. Returns the responses by the pool's keys;
     * a request without a response yields its ConnectionException instead of throwing.
     *
     * ```php
     * $responses = Http::pool(fn (Pool $pool) => [
     *     $pool->as('users')->get('https://api.test/users'),
     *     $pool->as('orders')->get('https://api.test/orders'),
     * ]);
     * ```
     *
     * @param callable(Pool): mixed $requests
     * @return array<array-key, Response|ConnectionException>
     */
    public function pool(callable $requests): array
    {
        $pool = new Pool($this);
        $requests($pool);
        $outgoing = [];
        $builders = [];
        foreach ($pool->entries() as $key => [$builder, $method, $url, $data, $query]) {
            $outgoing[$key] = $builder->build($method, $url, $data, $query);
            $builders[$key] = $builder;
        }
        $results = [];
        foreach ($this->transport->sendConcurrently($outgoing) as $key => $outcome) {
            $builder = $builders[$key];
            // Failed attempts of a retrying request are retried one by one.
            if ($builder->tries > 1 && $builder->shouldRetry($outcome, $outgoing[$key])) {
                try {
                    $outcome = $builder->retryRemaining($outgoing[$key], $outcome);
                } catch (RequestException $failure) {
                    $outcome = $failure->response;
                }
            }
            $results[$key] = $outcome;
        }
        return $results;
    }

    /**
     * @param array<array-key, mixed> $data
     * @param array<string, mixed> $query
     * @internal
     */
    public function build(string $method, string $url, array $data = [], array $query = []): OutgoingRequest
    {
        $method = strtoupper($method);
        if ($this->baseUrl !== '' && !preg_match('#^https?://#i', $url)) {
            $url = $this->baseUrl . '/' . ltrim($url, '/');
        }
        $headers = $this->headers;
        $body = '';
        $bodyless = $method === 'GET' || $method === 'HEAD';
        if ($bodyless) {
            $query = $data + $query;
        }
        $query += $this->query;
        if ($query !== []) {
            $url .= (str_contains($url, '?') ? '&' : '?') . http_build_query($query, '', '&', PHP_QUERY_RFC3986);
        }
        if ($this->rawBody !== null) {
            $body = $this->rawBody;
        } elseif (!$bodyless && $data !== []) {
            if ($this->format === 'form') {
                $body = http_build_query($data, '', '&');
                $headers['content-type'] ??= 'application/x-www-form-urlencoded';
            } else {
                $body = json_encode($data, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
                $headers['content-type'] ??= 'application/json';
            }
        }
        return new OutgoingRequest($method, $url, $headers, $body, $this->timeout, $this->verifyTls, $this->maxRedirects);
    }

    private function retryRemaining(OutgoingRequest $request, Response|ConnectionException $outcome): Response|ConnectionException
    {
        for ($attempt = 2; $attempt <= $this->tries; ++$attempt) {
            self::sleep(($this->backoff)($attempt - 1, $outcome));
            try {
                $outcome = $this->transport->send($request);
            } catch (ConnectionException $failure) {
                $outcome = $failure;
            }
            if (!$this->shouldRetry($outcome, $request)) {
                break;
            }
        }
        return $outcome instanceof ConnectionException ? $outcome : $this->finish($outcome, true);
    }

    private function shouldRetry(Response|ConnectionException $outcome, OutgoingRequest $request): bool
    {
        if ($this->retryWhen !== null) {
            return ($this->retryWhen)($outcome, $request);
        }
        return $outcome instanceof ConnectionException || $outcome->serverError() || $outcome->status() === 429;
    }

    private function finish(Response|ConnectionException $outcome, bool $retried): Response
    {
        if ($outcome instanceof ConnectionException) {
            throw $outcome;
        }
        if ($outcome->failed() && ($this->throwOnFailure || ($retried && $this->throwAfterRetries))) {
            throw new RequestException($outcome);
        }
        return $outcome;
    }

    private static function sleep(int $milliseconds): void
    {
        if ($milliseconds <= 0) {
            return;
        }
        if (\Fiber::getCurrent() !== null && function_exists('Pam\\Async\\delay')) {
            \Pam\Async\delay($milliseconds / 1000);
            return;
        }
        usleep($milliseconds * 1000);
    }

    private function with(string $property, mixed $value): self
    {
        $copy = clone $this;
        $copy->{$property} = $value;
        return $copy;
    }
}
