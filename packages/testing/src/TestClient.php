<?php

declare(strict_types=1);

namespace Pam\Testing;

use Pam\Contracts\Http\ApplicationInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class TestClient
{
    public function __construct(private ApplicationInterface $application)
    {
    }

    /** @param array<string, string|list<string>> $headers */
    public function request(
        string $method,
        string $target,
        array $headers = [],
        string $body = '',
    ): TestResponse {
        $normalizedHeaders = [];
        foreach ($headers as $name => $values) {
            $normalizedHeaders[strtolower($name)] = is_array($values) ? $values : [$values];
        }
        $path = parse_url($target, PHP_URL_PATH);
        if (!is_string($path) || $path === '' || $path[0] !== '/') {
            throw new \InvalidArgumentException('Test request targets must contain an absolute path.');
        }
        $parsedQuery = [];
        $queryString = parse_url($target, PHP_URL_QUERY);
        if (is_string($queryString)) {
            parse_str($queryString, $parsedQuery);
        }
        $query = [];
        foreach ($parsedQuery as $key => $value) {
            if (is_string($key)) {
                $query[$key] = $value;
            }
        }

        $response = $this->application->handle(
            new Request(strtoupper($method), $path, $query, $normalizedHeaders, $body),
            new Response(),
        );
        $exported = $response->export();

        return new TestResponse(
            $exported['status'],
            $exported['headers'],
            $exported['body'],
            $exported['chunks'],
        );
    }

    /** @param array<string, string|list<string>> $headers */
    public function get(string $target, array $headers = []): TestResponse
    {
        return $this->request('GET', $target, $headers);
    }

    /**
     * @param array<array-key, mixed> $payload
     * @param array<string, string|list<string>> $headers
     */
    public function postJson(string $target, array $payload, array $headers = []): TestResponse
    {
        $headers['content-type'] = 'application/json';
        return $this->request(
            'POST',
            $target,
            $headers,
            json_encode($payload, JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE),
        );
    }

}
