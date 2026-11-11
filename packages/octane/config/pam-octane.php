<?php

declare(strict_types=1);

return [
    'host' => env('PAM_HOST', '127.0.0.1'),
    'port' => (int) env('PAM_PORT', 8000),

    // The `octane` cache store in shared memory (one cache for every worker,
    // like on Swoole, sized by `octane.cache`). null: shared when task
    // workers run (--task-workers), per worker otherwise.
    'shared_cache' => env('PAM_OCTANE_SHARED_CACHE') === null ? null : (bool) env('PAM_OCTANE_SHARED_CACHE'),

    // Headers added to static files served from public/, by path pattern.
    'static_file_headers' => [
        // 'build/*' => ['Cache-Control' => 'public, max-age=31536000, immutable'],
    ],

    'server' => [
        // Serve files from public/ (Vite builds, robots.txt, storage:link).
        'serveStaticFiles' => (bool) env('PAM_SERVE_STATIC_FILES', true),
        // Bodies held in memory (JSON, forms). Multipart uploads are streamed
        // to temporary files and bounded by PHP's upload_max_filesize and
        // post_max_size instead, like PHP-FPM.
        'maxBodyBytes' => (int) env('PAM_MAX_BODY_BYTES', 2 * 1024 * 1024),
        'maxHeaderBytes' => (int) env('PAM_MAX_HEADER_BYTES', 32 * 1024),
        'maxHeaders' => (int) env('PAM_MAX_HEADERS', 100),
        'maxResponseBytes' => (int) env('PAM_MAX_RESPONSE_BYTES', 256 * 1024 * 1024),
        'maxResponseChunkBytes' => (int) env('PAM_MAX_RESPONSE_CHUNK_BYTES', 1024 * 1024),
        'responseStreamQueueCapacity' => (int) env('PAM_STREAM_QUEUE_CAPACITY', 16),
        // Defaults to Octane's `octane.max_execution_time` (0 disables it).
        // A request past it is answered 504 and its worker is replaced.
        'requestTimeoutMs' => env('PAM_REQUEST_TIMEOUT_MS') !== null ? (int) env('PAM_REQUEST_TIMEOUT_MS') : null,
        'tlsCert' => env('PAM_TLS_CERT') ?: null,
        'tlsKey' => env('PAM_TLS_KEY') ?: null,
        'maxConcurrentRequests' => 1,
        'phpExecutorQueueCapacity' => (int) env('PAM_PHP_QUEUE_CAPACITY', 1_024),
        'gcCollectCyclesEvery' => (int) env('PAM_GC_COLLECT_CYCLES_EVERY', 256),
        'gcMemCachesEvery' => (int) env('PAM_GC_MEM_CACHES_EVERY', 1_024),
        'leakDetectionSampleRate' => (int) env('PAM_LEAK_DETECTION_SAMPLE_RATE', 1_024),
        'routeMetrics' => (bool) env('PAM_ROUTE_METRICS', false),
        'routeMetricsMaxEntries' => (int) env('PAM_ROUTE_METRICS_MAX_ENTRIES', 256),
        'responseCachePaths' => array_values(array_filter(array_map(
            'trim',
            explode(',', (string) env('PAM_RESPONSE_CACHE_PATHS', '')),
        ))),
        'responseCacheVaryHeaders' => array_values(array_filter(array_map(
            'trim',
            explode(',', (string) env('PAM_RESPONSE_CACHE_VARY_HEADERS', 'accept-language')),
        ))),
        'responseCacheTtlMs' => (int) env('PAM_RESPONSE_CACHE_TTL_MS', 30_000),
        'responseCacheStaleWhileRevalidateMs' => (int) env('PAM_RESPONSE_CACHE_STALE_WHILE_REVALIDATE_MS', 0),
        'responseCacheMaxEntries' => (int) env('PAM_RESPONSE_CACHE_MAX_ENTRIES', 1_024),
        'responseCacheMaxBytes' => (int) env('PAM_RESPONSE_CACHE_MAX_BYTES', 64 * 1024 * 1024),
        'responseCachePurgePath' => env('PAM_RESPONSE_CACHE_PURGE_PATH') ?: null,
        'responseCachePurgeSecret' => env('PAM_RESPONSE_CACHE_PURGE_SECRET') ?: null,
        'responseCacheTagHeader' => env('PAM_RESPONSE_CACHE_TAG_HEADER', 'x-pam-cache-tags'),
        // Edge page cache: responses marked with `Cache-Control: public,
        // s-maxage=N` (see Pam\Octane\Http\Middleware\CacheResponse) are
        // served by Rust without entering PHP until they expire or are purged
        // (Pam::purge('products'), `pam cache:purge --tag=products`).
        'responseCache' => (bool) env('PAM_RESPONSE_CACHE', false),
        // Browsers send the session cookie to every page; allow it only when
        // the cached routes run without the session middleware
        // (->withoutMiddleware(CacheResponse::STATEFUL)).
        'responseCacheAllowCookies' => (bool) env('PAM_RESPONSE_CACHE_ALLOW_COOKIES', false),
        'responseCacheAllowAuthorization' => (bool) env('PAM_RESPONSE_CACHE_ALLOW_AUTHORIZATION', false),
        'responseCacheIgnoredQueryParams' => array_values(array_filter(array_map(
            'trim',
            explode(',', (string) env('PAM_RESPONSE_CACHE_IGNORED_QUERY_PARAMS', 'utm_*,gclid,fbclid')),
        ))),
        'responseCacheMaxObjectBytes' => (int) env('PAM_RESPONSE_CACHE_MAX_OBJECT_BYTES', 8 * 1024 * 1024),
        'responseCacheCompress' => (bool) env('PAM_RESPONSE_CACHE_COMPRESS', true),
        'responseCacheStatusHeader' => (bool) env('PAM_RESPONSE_CACHE_STATUS_HEADER', true),
        // Purges from queue workers, Artisan and `pam cache:purge` reach every
        // worker through this append-only log.
        'responseCacheInvalidationLog' => env(
            'PAM_RESPONSE_CACHE_INVALIDATION_LOG',
            function_exists('storage_path') ? storage_path('framework/cache/pam-page-cache.log') : null,
        ),
    ],
];
