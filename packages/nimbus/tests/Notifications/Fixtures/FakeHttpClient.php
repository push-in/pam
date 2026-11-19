<?php

declare(strict_types=1);

namespace Nimbus\Tests\Notifications\Fixtures;

use Nimbus\Notifications\Http\HttpClient;
use Nimbus\Notifications\Http\HttpRequest;
use Nimbus\Notifications\Http\HttpResponse;
use Nimbus\Notifications\Http\HttpTransportException;

final class FakeHttpClient implements HttpClient
{
    /** @var list<HttpRequest> */
    public array $requests = [];

    /** @param list<HttpResponse|HttpTransportException|(\Closure(HttpRequest): HttpResponse)> $responses */
    public function __construct(private array $responses = [])
    {
    }

    public function push(HttpResponse|HttpTransportException|\Closure $response): self
    {
        $this->responses[] = $response;
        return $this;
    }

    public function send(HttpRequest $request): HttpResponse
    {
        $this->requests[] = $request;
        $response = array_shift($this->responses) ?? new HttpResponse(200, [], '{}');
        if ($response instanceof HttpTransportException) {
            throw $response;
        }
        return $response instanceof \Closure ? $response($request) : $response;
    }
}
