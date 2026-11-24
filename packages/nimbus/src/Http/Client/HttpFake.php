<?php

declare(strict_types=1);

namespace Nimbus\Http\Client;

/**
 * Records outgoing requests and answers them with stubs, for tests.
 *
 * ```php
 * $http = Http::fake([
 *     'api.github.com/users/*' => ['login' => 'octocat'],            // JSON, 200
 *     'api.example.com/flaky' => Http::sequence()->pushStatus(503)->push(['ok' => true]),
 *     '*' => Http::response('', 404),
 * ]);
 * // ... exercise the code ...
 * $http->assertSent(fn (OutgoingRequest $request) => $request->hasHeader('authorization'));
 * ```
 *
 * Patterns match the full URL, `*` matching anything and a leading `*` implied. A request
 * that matches no stub gets an empty 200 response, or throws after preventStrayRequests().
 */
final class HttpFake implements Transport
{
    /** @var list<array{string, Response|ResponseSequence|ConnectionException|\Closure}> */
    private array $stubs = [];

    /** @var list<array{OutgoingRequest, Response|ConnectionException}> */
    private array $recorded = [];

    private bool $preventStrayRequests = false;

    /** @param array<string, Response|ResponseSequence|ConnectionException|\Closure|array<array-key, mixed>|string>|\Closure $stubs */
    public function __construct(array|\Closure $stubs = [])
    {
        $this->stub($stubs);
    }

    /**
     * Adds stubs: a URL pattern => a Response, a ResponseSequence, a ConnectionException, an
     * array (JSON body), a string (body) or a closure receiving the OutgoingRequest. A closure
     * alone answers every URL.
     *
     * @param array<string, Response|ResponseSequence|ConnectionException|\Closure|array<array-key, mixed>|string>|\Closure $stubs
     */
    public function stub(array|\Closure $stubs): self
    {
        if ($stubs instanceof \Closure) {
            $stubs = ['*' => $stubs];
        }
        foreach ($stubs as $pattern => $stub) {
            $this->stubs[] = [
                self::regex($pattern),
                $stub instanceof Response || $stub instanceof ResponseSequence || $stub instanceof ConnectionException || $stub instanceof \Closure
                    ? $stub
                    : Http::response($stub),
            ];
        }
        return $this;
    }

    /** Requests without a matching stub throw instead of getting an empty 200 response. */
    public function preventStrayRequests(bool $prevent = true): self
    {
        $this->preventStrayRequests = $prevent;
        return $this;
    }

    public function send(OutgoingRequest $request): Response
    {
        try {
            $response = $this->respond($request)->withRequest($request);
        } catch (ConnectionException $failure) {
            $this->recorded[] = [$request, $failure];
            throw $failure;
        }
        $this->recorded[] = [$request, $response];
        return $response;
    }

    public function sendConcurrently(array $requests): array
    {
        $responses = [];
        foreach ($requests as $key => $request) {
            try {
                $responses[$key] = $this->send($request);
            } catch (ConnectionException $failure) {
                $responses[$key] = $failure;
            }
        }
        return $responses;
    }

    /** @param \Closure(OutgoingRequest, Response|ConnectionException): bool $callback */
    public function assertSent(\Closure $callback): void
    {
        self::assert($this->recorded($callback) !== [], 'An expected HTTP request was not sent.');
    }

    /** @param \Closure(OutgoingRequest, Response|ConnectionException): bool $callback */
    public function assertNotSent(\Closure $callback): void
    {
        self::assert($this->recorded($callback) === [], 'An unexpected HTTP request was sent.');
    }

    public function assertSentCount(int $count): void
    {
        $actual = count($this->recorded);
        self::assert($actual === $count, "{$actual} HTTP requests were sent instead of {$count}.");
    }

    public function assertNothingSent(): void
    {
        self::assert($this->recorded === [], count($this->recorded) . ' unexpected HTTP requests were sent.');
    }

    /**
     * Sent requests with their responses, optionally filtered.
     *
     * @param (\Closure(OutgoingRequest, Response|ConnectionException): bool)|null $callback
     * @return list<array{OutgoingRequest, Response|ConnectionException}>
     */
    public function recorded(?\Closure $callback = null): array
    {
        if ($callback === null) {
            return $this->recorded;
        }
        return array_values(array_filter(
            $this->recorded,
            static fn (array $pair): bool => $callback($pair[0], $pair[1]) === true,
        ));
    }

    private function respond(OutgoingRequest $request): Response
    {
        foreach ($this->stubs as [$pattern, $stub]) {
            if (preg_match($pattern, $request->url) !== 1) {
                continue;
            }
            if ($stub instanceof \Closure) {
                $result = $stub($request);
                if ($result === null) {
                    continue;
                }
                if ($result instanceof ConnectionException) {
                    throw $result;
                }
                if ($result instanceof ResponseSequence) {
                    return $result->next();
                }
                return $result instanceof Response ? $result : Http::response(is_array($result) || is_string($result) ? $result : null);
            }
            if ($stub instanceof ConnectionException) {
                throw $stub;
            }
            return $stub instanceof ResponseSequence ? $stub->next() : $stub;
        }
        if ($this->preventStrayRequests) {
            throw new \LogicException("Attempted request to [{$request->url}] without a matching fake.");
        }
        return new Response(200);
    }

    private static function regex(string $pattern): string
    {
        $pattern = str_starts_with($pattern, '*') ? $pattern : '*' . $pattern;
        return '#^' . str_replace('\*', '.*', preg_quote($pattern, '#')) . '$#Ds';
    }

    private static function assert(bool $condition, string $message): void
    {
        if (class_exists(\PHPUnit\Framework\Assert::class)) {
            \PHPUnit\Framework\Assert::assertTrue($condition, $message);
            return;
        }
        if (!$condition) {
            throw new \LogicException($message);
        }
    }
}
