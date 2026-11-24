<?php

declare(strict_types=1);

namespace Nimbus\Http\Client;

/**
 * Static entry point of the HTTP client, one factory per worker.
 *
 * ```php
 * use Nimbus\Http\Client\Http;
 *
 * $response = Http::withToken($token)->retry(3, 100)->post('https://api.test/orders', ['sku' => 'A1']);
 * $response->throw()->json('data.id');
 * ```
 *
 * Inside the PAM runtime requests suspend only the request Fiber; elsewhere they go
 * through symfony/http-client. Tests call `Http::fake()`.
 */
final class Http
{
    private static ?Factory $factory = null;

    private function __construct()
    {
    }

    public static function factory(): Factory
    {
        return self::$factory ??= new Factory();
    }

    /** Replaces the worker's factory; null restores the detected transport. */
    public static function swap(?Factory $factory): void
    {
        self::$factory = $factory;
    }

    /** @param array<string, Response|ResponseSequence|ConnectionException|\Closure|array<array-key, mixed>|string>|\Closure $stubs */
    public static function fake(array|\Closure $stubs = []): HttpFake
    {
        return self::factory()->fake($stubs);
    }

    /**
     * A stub response: arrays become a JSON body.
     *
     * @param array<array-key, mixed>|string|null $body
     * @param array<string, string|list<string>> $headers
     */
    public static function response(array|string|null $body = null, int $status = 200, array $headers = []): Response
    {
        if (is_array($body)) {
            $headers += ['content-type' => 'application/json'];
            $body = json_encode($body, JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
        }
        return new Response($status, $body ?? '', $headers);
    }

    public static function sequence(): ResponseSequence
    {
        return new ResponseSequence();
    }

    public static function failedConnection(string $message = 'Connection refused.'): ConnectionException
    {
        return new ConnectionException($message);
    }

    public static function request(): PendingRequest
    {
        return self::factory()->request();
    }

    public static function baseUrl(string $url): PendingRequest
    {
        return self::request()->baseUrl($url);
    }

    /** @param array<string, string> $headers */
    public static function withHeaders(array $headers): PendingRequest
    {
        return self::request()->withHeaders($headers);
    }

    public static function withToken(#[\SensitiveParameter] string $token, string $type = 'Bearer'): PendingRequest
    {
        return self::request()->withToken($token, $type);
    }

    public static function withBasicAuth(string $username, #[\SensitiveParameter] string $password): PendingRequest
    {
        return self::request()->withBasicAuth($username, $password);
    }

    public static function acceptJson(): PendingRequest
    {
        return self::request()->acceptJson();
    }

    public static function asJson(): PendingRequest
    {
        return self::request()->asJson();
    }

    public static function asForm(): PendingRequest
    {
        return self::request()->asForm();
    }

    public static function withBody(string $body, string $contentType = 'application/json'): PendingRequest
    {
        return self::request()->withBody($body, $contentType);
    }

    public static function timeout(float $seconds): PendingRequest
    {
        return self::request()->timeout($seconds);
    }

    /**
     * @param int|list<int>|(\Closure(int, Response|ConnectionException): int) $sleepMilliseconds
     * @param (\Closure(Response|ConnectionException, OutgoingRequest): bool)|null $when
     */
    public static function retry(int $times, int|array|\Closure $sleepMilliseconds = 0, ?\Closure $when = null, bool $throw = true): PendingRequest
    {
        return self::request()->retry($times, $sleepMilliseconds, $when, $throw);
    }

    /** @param array<string, mixed> $query */
    public static function get(string $url, array $query = []): Response
    {
        return self::request()->get($url, $query);
    }

    /** @param array<string, mixed> $query */
    public static function head(string $url, array $query = []): Response
    {
        return self::request()->head($url, $query);
    }

    /** @param array<array-key, mixed> $data */
    public static function post(string $url, array $data = []): Response
    {
        return self::request()->post($url, $data);
    }

    /** @param array<array-key, mixed> $data */
    public static function put(string $url, array $data = []): Response
    {
        return self::request()->put($url, $data);
    }

    /** @param array<array-key, mixed> $data */
    public static function patch(string $url, array $data = []): Response
    {
        return self::request()->patch($url, $data);
    }

    /** @param array<array-key, mixed> $data */
    public static function delete(string $url, array $data = []): Response
    {
        return self::request()->delete($url, $data);
    }

    /**
     * @param callable(Pool): mixed $requests
     * @return array<array-key, Response|ConnectionException>
     */
    public static function pool(callable $requests): array
    {
        return self::request()->pool($requests);
    }
}
