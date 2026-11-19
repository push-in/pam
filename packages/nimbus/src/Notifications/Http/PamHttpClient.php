<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Http;

/**
 * The PAM runtime HTTP client: non-blocking inside request Fibers, HTTP/1.1 only.
 * HTTP/2 requests (APNs) go to the fallback client.
 */
final class PamHttpClient implements HttpClient
{
    /** @var array<string, \Pam\Http\Client> */
    private array $clients = [];

    public function __construct(private readonly ?HttpClient $http2 = null)
    {
        if (!class_exists(\Pam\Http\Client::class)) {
            throw new \LogicException('PamHttpClient requires the PAM runtime.');
        }
    }

    public function send(HttpRequest $request): HttpResponse
    {
        if ($request->http2) {
            if ($this->http2 === null) {
                throw new \LogicException('The PAM HTTP client does not speak HTTP/2; configure a fallback client.');
            }
            return $this->http2->send($request);
        }
        $key = 't' . sprintf('%.3F', $request->timeout);
        $client = $this->clients[$key] ??= new \Pam\Http\Client(timeout: $request->timeout, maxResponseBytes: 1_048_576, maxRedirects: 0);
        try {
            $response = $client->request($request->method, $request->url, $request->headers, $request->body);
        } catch (\InvalidArgumentException $invalid) {
            throw $invalid;
        } catch (\Throwable $failure) {
            throw new HttpTransportException('HTTP request failed: ' . $failure->getMessage(), 0, $failure);
        }
        $headers = [];
        foreach ($response->headers as $name => $values) {
            $headers[$name] = implode(', ', $values);
        }
        return new HttpResponse($response->status, $headers, $response->body);
    }
}
