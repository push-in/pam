<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Http;

/**
 * ext-curl transport with HTTP/2 (required by APNs). One handle is reused per worker,
 * so connections stay open between deliveries. curl blocks the worker while it waits.
 */
final class CurlHttpClient implements HttpClient
{
    private const int MAXIMUM_RESPONSE_BYTES = 1_048_576;

    private ?\CurlHandle $handle = null;

    public function __construct()
    {
        if (!extension_loaded('curl')) {
            throw new \LogicException('CurlHttpClient requires ext-curl.');
        }
    }

    public function send(HttpRequest $request): HttpResponse
    {
        $handle = $this->handle ??= curl_init() ?: throw new HttpTransportException('curl_init() failed.');
        curl_reset($handle);
        $headers = [];
        foreach ($request->headers as $name => $value) {
            $headers[] = "{$name}: {$value}";
        }
        $responseHeaders = [];
        $body = '';
        /** @var array<int, mixed> $options */
        $options = [
            CURLOPT_URL => $request->url,
            CURLOPT_CUSTOMREQUEST => $request->method,
            CURLOPT_HTTPHEADER => $headers,
            CURLOPT_TIMEOUT_MS => (int) ceil($request->timeout * 1000),
            CURLOPT_CONNECTTIMEOUT_MS => (int) ceil(min($request->timeout, 10.0) * 1000),
            CURLOPT_FOLLOWLOCATION => false,
            CURLOPT_PROTOCOLS => CURLPROTO_HTTP | CURLPROTO_HTTPS,
            CURLOPT_HTTP_VERSION => $request->http2 ? CURL_HTTP_VERSION_2_0 : CURL_HTTP_VERSION_1_1,
            CURLOPT_HEADERFUNCTION => static function (\CurlHandle $curl, string $line) use (&$responseHeaders): int {
                $parts = explode(':', $line, 2);
                if (count($parts) === 2) {
                    $responseHeaders[strtolower(trim($parts[0]))] = trim($parts[1]);
                }
                return strlen($line);
            },
            CURLOPT_WRITEFUNCTION => static function (\CurlHandle $curl, string $chunk) use (&$body): int {
                if (strlen($body) + strlen($chunk) > self::MAXIMUM_RESPONSE_BYTES) {
                    return 0;
                }
                $body .= $chunk;
                return strlen($chunk);
            },
        ];
        curl_setopt_array($handle, $options);
        if ($request->body !== '' || $request->method === 'POST') {
            curl_setopt($handle, CURLOPT_POSTFIELDS, $request->body);
        }
        if (curl_exec($handle) === false) {
            throw new HttpTransportException('HTTP request failed: ' . curl_error($handle));
        }
        /** @var array<string, string> $responseHeaders */
        return new HttpResponse((int) curl_getinfo($handle, CURLINFO_RESPONSE_CODE), $responseHeaders, $body);
    }
}
