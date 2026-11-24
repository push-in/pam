<?php

declare(strict_types=1);

namespace Nimbus\Http\Client;

/**
 * Transport over the PAM runtime client (`Pam\Http\Client`). Inside a request Fiber a
 * request suspends only that Fiber, so the worker keeps serving other requests while it
 * waits; concurrent requests run in sibling Fibers through `Pam\Async\async()`.
 * HTTP/1.1 only.
 */
final class PamTransport implements Transport
{
    /** @var array<string, \Pam\Http\Client> */
    private array $clients = [];

    public function __construct(private readonly int $maxResponseBytes = 16 * 1024 * 1024)
    {
        if (!class_exists(\Pam\Http\Client::class)) {
            throw new \LogicException('PamTransport requires the PAM runtime.');
        }
        if ($maxResponseBytes < 1) {
            throw new \InvalidArgumentException('Maximum response bytes must be positive.');
        }
    }

    public static function available(): bool
    {
        return class_exists('Pam\Internal\Runtime', false) && class_exists(\Pam\Http\Client::class);
    }

    public function send(OutgoingRequest $request): Response
    {
        $key = sprintf('%.3F|%d|%d', $request->timeout, (int) $request->verifyTls, $request->maxRedirects);
        $client = $this->clients[$key] ??= new \Pam\Http\Client(
            timeout: $request->timeout,
            maxResponseBytes: $this->maxResponseBytes,
            maxRedirects: $request->maxRedirects,
            verifyTls: $request->verifyTls,
        );
        try {
            $response = $client->request($request->method, $request->url, $request->headers, $request->body);
        } catch (\InvalidArgumentException $invalid) {
            throw $invalid;
        } catch (\Throwable $failure) {
            if ($failure::class === 'Pam\Internal\HttpRequestCancelled') {
                throw $failure;
            }
            throw new ConnectionException('HTTP request failed: ' . $failure->getMessage(), 0, $failure);
        }
        return new Response($response->status, $response->body, $response->headers, $request);
    }

    public function sendConcurrently(array $requests): array
    {
        $responses = [];
        if (\Fiber::getCurrent() === null || !function_exists('Pam\\Async\\async') || !function_exists('Pam\\Async\\await')) {
            foreach ($requests as $key => $request) {
                $responses[$key] = $this->attempt($request);
            }
            return $responses;
        }
        $futures = [];
        foreach ($requests as $key => $request) {
            $futures[$key] = \Pam\Async\async(fn (): Response|ConnectionException => $this->attempt($request));
        }
        foreach ($futures as $key => $future) {
            $result = \Pam\Async\await($future);
            $responses[$key] = $result instanceof Response || $result instanceof ConnectionException
                ? $result
                : new ConnectionException('The concurrent HTTP request returned no response.');
        }
        return $responses;
    }

    private function attempt(OutgoingRequest $request): Response|ConnectionException
    {
        try {
            return $this->send($request);
        } catch (ConnectionException $failure) {
            return $failure;
        }
    }
}
