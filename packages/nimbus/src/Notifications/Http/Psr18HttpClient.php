<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Http;

use Psr\Http\Client\ClientExceptionInterface;
use Psr\Http\Client\ClientInterface;
use Psr\Http\Message\RequestFactoryInterface;
use Psr\Http\Message\StreamFactoryInterface;

/**
 * Adapter for any PSR-18 client. Timeouts and HTTP/2 are configured on the client itself.
 */
final readonly class Psr18HttpClient implements HttpClient
{
    public function __construct(
        private ClientInterface $client,
        private RequestFactoryInterface $requests,
        private StreamFactoryInterface $streams,
    ) {
    }

    public function send(HttpRequest $request): HttpResponse
    {
        $psr = $this->requests->createRequest($request->method, $request->url);
        foreach ($request->headers as $name => $value) {
            $psr = $psr->withHeader($name, $value);
        }
        if ($request->http2) {
            $psr = $psr->withProtocolVersion('2.0');
        }
        if ($request->body !== '') {
            $psr = $psr->withBody($this->streams->createStream($request->body));
        }
        try {
            $response = $this->client->sendRequest($psr);
        } catch (ClientExceptionInterface $failure) {
            throw new HttpTransportException('HTTP request failed: ' . $failure->getMessage(), 0, $failure);
        }
        $headers = [];
        foreach (array_keys($response->getHeaders()) as $name) {
            $headers[(string) $name] = $response->getHeaderLine((string) $name);
        }
        return new HttpResponse($response->getStatusCode(), $headers, (string) $response->getBody());
    }
}
