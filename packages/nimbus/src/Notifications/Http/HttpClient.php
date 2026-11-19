<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Http;

/**
 * Transport of the push and webhook channels; tests replace it with a fake.
 */
interface HttpClient
{
    /** @throws HttpTransportException When no response was received. */
    public function send(HttpRequest $request): HttpResponse;
}
