<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Http;

/**
 * Picks the default transport: the PAM client inside the runtime (curl for HTTP/2), else curl.
 * Pass a `Psr18HttpClient` explicitly to use a PSR-18 client.
 */
final class HttpClients
{
    private function __construct()
    {
    }

    public static function detect(): HttpClient
    {
        $curl = extension_loaded('curl') ? new CurlHttpClient() : null;
        if (class_exists('Pam\Internal\Runtime', false) && class_exists(\Pam\Http\Client::class)) {
            return new PamHttpClient($curl);
        }
        return $curl ?? throw new \LogicException('No HTTP client: run inside PAM, install ext-curl or pass a PSR-18 client.');
    }
}
