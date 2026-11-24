<?php

declare(strict_types=1);

namespace Nimbus\Http\Client;

/**
 * Sends outgoing requests. `PamTransport` suspends only the request Fiber inside the PAM
 * runtime; `SymfonyTransport` uses symfony/http-client elsewhere; `HttpFake` records them.
 */
interface Transport
{
    /** @throws ConnectionException When no response was received. */
    public function send(OutgoingRequest $request): Response;

    /**
     * Sends the requests concurrently where the transport can.
     *
     * @template TKey of array-key
     * @param array<TKey, OutgoingRequest> $requests
     * @return array<TKey, Response|ConnectionException>
     */
    public function sendConcurrently(array $requests): array;
}
