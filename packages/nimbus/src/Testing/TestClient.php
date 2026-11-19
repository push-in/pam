<?php

declare(strict_types=1);

namespace Nimbus\Testing;

use Nimbus\App;
use Nimbus\Auth\Authenticator;
use Nimbus\Auth\Principal;
use Pam\Http\Request;
use Pam\Http\Response;

/**
 * Sends requests to an application in memory. `with*()` return a new client; `actingAs()`
 * rebinds the application's Authenticator.
 */
final readonly class TestClient
{
    /** @param array<string, string> $headers Sent with every request. */
    public function __construct(
        private App $app,
        private array $headers = [],
    ) {
    }

    /** @param array<string, string> $headers */
    public function withHeaders(array $headers): self
    {
        return new self($this->app, array_replace($this->headers, self::normalize($headers)));
    }

    public function withHeader(string $name, string $value): self
    {
        return $this->withHeaders([$name => $value]);
    }

    public function withToken(string $token, string $type = 'Bearer'): self
    {
        return $this->withHeader('authorization', "{$type} {$token}");
    }

    /**
     * Authenticates every following request as the principal: the container's Authenticator
     * becomes one that returns it, so `AuthenticateMiddleware` resolved through the container,
     * `auth()` routes and `can()` checks see it.
     */
    public function actingAs(Principal $principal): self
    {
        $this->app->container()->instance(Authenticator::class, new ActingAsAuthenticator($principal));
        return $this;
    }

    /**
     * @param array<string, mixed> $query Merged over the query string of the path.
     * @param array<string, string> $headers
     */
    public function request(
        string $method,
        string $path,
        array $query = [],
        array $headers = [],
        string $body = '',
    ): TestResponse {
        $queryString = parse_url($path, PHP_URL_QUERY);
        if (is_string($queryString)) {
            parse_str($queryString, $parsed);
            foreach ($parsed as $key => $value) {
                $query += [(string) $key => $value];
            }
            $path = substr($path, 0, (int) strpos($path, '?'));
        }
        $normalized = [];
        foreach (array_replace($this->headers, self::normalize($headers)) as $name => $value) {
            $normalized[$name] = [$value];
        }
        $request = new Request(strtoupper($method), $path, $query, $normalized, $body);
        return new TestResponse($this->app->handle($request, new Response())->export());
    }

    /**
     * A JSON request: the payload is the body, `accept` and `content-type` are JSON.
     *
     * @param array<array-key, mixed>|null $payload
     * @param array<string, string> $headers
     */
    public function json(string $method, string $path, ?array $payload = null, array $headers = []): TestResponse
    {
        $headers = self::normalize($headers) + ['accept' => 'application/json'];
        $body = '';
        if ($payload !== null) {
            $headers += ['content-type' => 'application/json'];
            $body = json_encode($payload, JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES);
        }
        return $this->request($method, $path, headers: $headers, body: $body);
    }

    /** @param array<string, string> $headers */
    public function get(string $path, array $headers = []): TestResponse
    {
        return $this->request('GET', $path, headers: $headers);
    }

    /** @param array<string, string> $headers */
    public function getJson(string $path, array $headers = []): TestResponse
    {
        return $this->json('GET', $path, null, $headers);
    }

    /**
     * @param array<array-key, mixed> $payload
     * @param array<string, string> $headers
     */
    public function postJson(string $path, array $payload = [], array $headers = []): TestResponse
    {
        return $this->json('POST', $path, $payload, $headers);
    }

    /**
     * @param array<array-key, mixed> $payload
     * @param array<string, string> $headers
     */
    public function putJson(string $path, array $payload = [], array $headers = []): TestResponse
    {
        return $this->json('PUT', $path, $payload, $headers);
    }

    /**
     * @param array<array-key, mixed> $payload
     * @param array<string, string> $headers
     */
    public function patchJson(string $path, array $payload = [], array $headers = []): TestResponse
    {
        return $this->json('PATCH', $path, $payload, $headers);
    }

    /**
     * @param array<array-key, mixed>|null $payload
     * @param array<string, string> $headers
     */
    public function deleteJson(string $path, ?array $payload = null, array $headers = []): TestResponse
    {
        return $this->json('DELETE', $path, $payload, $headers);
    }

    /**
     * @param array<string, string> $headers
     * @return array<string, string>
     */
    private static function normalize(array $headers): array
    {
        $normalized = [];
        foreach ($headers as $name => $value) {
            $normalized[strtolower($name)] = $value;
        }
        return $normalized;
    }
}
