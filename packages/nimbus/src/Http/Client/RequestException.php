<?php

declare(strict_types=1);

namespace Nimbus\Http\Client;

/** A response with a 4xx or 5xx status, thrown by `Response::throw()` and exhausted retries. */
class RequestException extends \RuntimeException
{
    public function __construct(public readonly Response $response)
    {
        parent::__construct(sprintf(
            'HTTP request returned status code %d%s',
            $response->status(),
            $response->body() === '' ? '.' : ': ' . mb_strimwidth($response->body(), 0, 200, '...'),
        ), $response->status());
    }
}
