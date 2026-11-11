<?php

declare(strict_types=1);

// Edge page cache fixture. Every response echoes everything the application
// could have personalised on (cookies, credentials, tenant, language, host,
// forwarded host, query) plus a per-worker call counter, so tests can prove
// a cached response is only ever served to requests it is valid for.
// The response's cacheability is driven by query parameters:
//   cc=<Cache-Control>  vary=<Vary>  tags=<Pam-Cache-Tags>  setcookie=1
//   status=<code>  delay=<ms>  size=<bytes of padding>  etag=<value>
//   fail=<n>  (answer 500 from the n-th call on)

use Pam\Http\Response;
use Pam\Http\Server;

$calls = 0;

Server::createNative(static function (Response $response) use (&$calls): Response {
    $path = parse_url($_SERVER['REQUEST_URI'] ?? '/', PHP_URL_PATH) ?: '/';
    // Readiness probe: not counted, so `calls` only counts test requests.
    if ($path === '/__ready') {
        return $response->json(['token' => getenv('PAM_PC_TOKEN') ?: null]);
    }
    $call = ++$calls;
    $query = $_GET;

    if ($path === '/purge') {
        $result = pam_cache_purge(
            isset($query['tag']) ? explode(',', (string) $query['tag']) : [],
            isset($query['url']) ? [(string) $query['url']] : [],
            isset($query['all']),
        );
        return $response
            ->header('Cache-Control', 'no-store')
            ->json(['purged' => $result]);
    }
    if ($path === '/purge-invalid') {
        try {
            pam_cache_purge(['bad tag!']);
            return $response->json(['error' => null]);
        } catch (\ValueError $error) {
            return $response->json(['error' => $error->getMessage()]);
        }
    }

    if (isset($query['delay'])) {
        \Pam\Async\delay(((int) $query['delay']) / 1000);
    }
    if (isset($query['fail']) && $call >= (int) $query['fail']) {
        return $response->json(['calls' => $call, 'error' => true], 500);
    }
    if (isset($query['cc'])) {
        $response->header('Cache-Control', (string) $query['cc']);
    }
    if (isset($query['vary'])) {
        $response->header('Vary', (string) $query['vary']);
    }
    if (isset($query['tags'])) {
        $response->header('Pam-Cache-Tags', (string) $query['tags']);
    }
    if (isset($query['etag'])) {
        $response->header('ETag', '"' . $query['etag'] . '"');
        // Like Laravel's static files and ETag middleware: PHP answers 304.
        if (($_SERVER['HTTP_IF_NONE_MATCH'] ?? null) === '"' . $query['etag'] . '"') {
            return $response->status(304)->send('');
        }
    }
    if (isset($query['setcookie'])) {
        $response->cookie('session', 'fresh-' . $call);
    }
    if (isset($query['status'])) {
        $response->status((int) $query['status']);
    }
    $response->header('Content-Type', 'application/json');

    return $response->send(json_encode([
        'calls' => $call,
        'worker' => getmypid(),
        'path' => $path,
        'query' => $query,
        'host' => $_SERVER['HTTP_HOST'] ?? null,
        'forwardedHost' => $_SERVER['HTTP_X_FORWARDED_HOST'] ?? null,
        'cookie' => $_SERVER['HTTP_COOKIE'] ?? null,
        'session' => $_COOKIE['session'] ?? null,
        'authorization' => $_SERVER['HTTP_AUTHORIZATION'] ?? null,
        'tenant' => $_SERVER['HTTP_X_TENANT'] ?? null,
        'language' => $_SERVER['HTTP_ACCEPT_LANGUAGE'] ?? null,
        'padding' => str_repeat('<p>cached page body</p>', (int) (($query['size'] ?? 0) / 23)),
    ], JSON_THROW_ON_ERROR));
})->listen((int) getenv('PAM_TEST_PORT'), '127.0.0.1', array_filter([
    'responseCache' => true,
    'responseCacheAllowCookies' => getenv('PAM_PC_ALLOW_COOKIES') === '1',
    'responseCacheAllowAuthorization' => getenv('PAM_PC_ALLOW_AUTHORIZATION') === '1',
    'responseCacheMaxBytes' => (int) (getenv('PAM_PC_MAX_BYTES') ?: 64 * 1024 * 1024),
    'responseCacheMaxEntries' => (int) (getenv('PAM_PC_MAX_ENTRIES') ?: 100_000),
    'responseCacheIgnoredQueryParams' => ['utm_*'],
    'responseCacheInvalidationLog' => getenv('PAM_PC_LOG') ?: null,
    'responseCacheCompress' => getenv('PAM_PC_COMPRESS') !== '0',
    'maxConcurrentRequests' => (int) (getenv('PAM_PC_CONCURRENCY') ?: 4096),
    'requestTimeoutMs' => 30_000,
], static fn (mixed $value): bool => $value !== null));
