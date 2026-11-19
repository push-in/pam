<?php

declare(strict_types=1);

namespace Tests;

use Nimbus\Testing\TestClient;
use Nimbus\Testing\TestResponse;

/** Keeps the cookies between requests, like a browser, and fills `_token` in forms. */
final class Browser
{
    private string $token = '';

    public function __construct(private TestClient $client)
    {
    }

    public function get(string $path): TestResponse
    {
        return $this->remember($this->client->get($path));
    }

    /** @param array<string, string> $form */
    public function post(string $path, array $form = []): TestResponse
    {
        if ($this->token === '') {
            $this->get('/login');
        }
        return $this->remember($this->client->post($path, $form + ['_token' => $this->token]));
    }

    /** @param array<string, string> $form Sent without the CSRF token. */
    public function postWithoutToken(string $path, array $form = []): TestResponse
    {
        return $this->remember($this->client->post($path, $form));
    }

    private function remember(TestResponse $response): TestResponse
    {
        $cookies = $response->cookies();
        if ($cookies !== []) {
            $this->client = $this->client->withCookiesFrom($response);
        }
        $this->token = $cookies['XSRF-TOKEN'] ?? $this->token;
        return $response;
    }
}
