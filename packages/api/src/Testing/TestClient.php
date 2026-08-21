<?php

declare(strict_types=1);

namespace Pam\Api\Testing;

use Pam\App;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class TestClient
{
    public function __construct(private App $app)
    {
    }

    /**
     * @param array<string, mixed> $query
     * @param array<string, string> $headers
     */
    public function request(
        string $method,
        string $path,
        array $query = [],
        array $headers = [],
        string $body = '',
    ): TestResponse {
        $normalized = [];
        foreach ($headers as $name => $value) {
            $normalized[strtolower($name)] = [$value];
        }
        $request = new Request(strtoupper($method), $path, $query, $normalized, $body);
        return new TestResponse($this->app->handle($request, new Response())->export());
    }

    /**
     * @param array<string, mixed> $payload
     * @param array<string, string> $headers
     */
    public function postJson(string $path, array $payload, array $headers = []): TestResponse
    {
        $headers['content-type'] = 'application/json';
        return $this->request(
            'POST',
            $path,
            headers: $headers,
            body: json_encode($payload, JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE),
        );
    }

    /** @param array<string, string> $headers */
    public function get(string $path, array $headers = []): TestResponse
    {
        return $this->request('GET', $path, headers: $headers);
    }
}
