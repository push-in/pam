<?php

declare(strict_types=1);

namespace Nimbus\Http\Client;

use Symfony\Component\HttpClient\HttpClient as SymfonyHttpClient;
use Symfony\Contracts\HttpClient\Exception\TransportExceptionInterface;
use Symfony\Contracts\HttpClient\HttpClientInterface;
use Symfony\Contracts\HttpClient\ResponseInterface;

/**
 * Transport over symfony/http-client (curl or native streams). It blocks the calling
 * process while it waits; concurrent requests are multiplexed by Symfony.
 */
final readonly class SymfonyTransport implements Transport
{
    private HttpClientInterface $client;

    public function __construct(?HttpClientInterface $client = null)
    {
        if ($client === null && !class_exists(SymfonyHttpClient::class)) {
            throw new \LogicException('SymfonyTransport requires symfony/http-client: composer require symfony/http-client.');
        }
        $this->client = $client ?? SymfonyHttpClient::create();
    }

    public function send(OutgoingRequest $request): Response
    {
        return $this->receive($request, $this->start($request));
    }

    public function sendConcurrently(array $requests): array
    {
        $started = [];
        foreach ($requests as $key => $request) {
            try {
                $started[$key] = $this->start($request);
            } catch (ConnectionException $failure) {
                $started[$key] = $failure;
            }
        }
        $responses = [];
        foreach ($started as $key => $response) {
            try {
                $responses[$key] = $response instanceof ConnectionException
                    ? $response
                    : $this->receive($requests[$key], $response);
            } catch (ConnectionException $failure) {
                $responses[$key] = $failure;
            }
        }
        return $responses;
    }

    private function start(OutgoingRequest $request): ResponseInterface
    {
        try {
            return $this->client->request($request->method, $request->url, [
                'headers' => $request->headers,
                'body' => $request->body,
                'timeout' => $request->timeout,
                'max_duration' => $request->timeout,
                'max_redirects' => $request->maxRedirects,
                'verify_peer' => $request->verifyTls,
                'verify_host' => $request->verifyTls,
            ]);
        } catch (TransportExceptionInterface $failure) {
            throw new ConnectionException($failure->getMessage(), 0, $failure);
        }
    }

    private function receive(OutgoingRequest $request, ResponseInterface $response): Response
    {
        try {
            return new Response(
                $response->getStatusCode(),
                $response->getContent(false),
                $response->getHeaders(false),
                $request,
            );
        } catch (TransportExceptionInterface $failure) {
            throw new ConnectionException($failure->getMessage(), 0, $failure);
        }
    }
}
