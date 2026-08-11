<?php

declare(strict_types=1);

return [
    'host' => env('PAM_HOST', '127.0.0.1'),
    'port' => (int) env('PAM_PORT', 8000),

    'server' => [
        'maxBodyBytes' => (int) env('PAM_MAX_BODY_BYTES', 2 * 1024 * 1024),
        'maxHeaderBytes' => (int) env('PAM_MAX_HEADER_BYTES', 32 * 1024),
        'maxHeaders' => (int) env('PAM_MAX_HEADERS', 100),
        'maxResponseBytes' => (int) env('PAM_MAX_RESPONSE_BYTES', 256 * 1024 * 1024),
        'maxResponseChunkBytes' => (int) env('PAM_MAX_RESPONSE_CHUNK_BYTES', 1024 * 1024),
        'responseStreamQueueCapacity' => (int) env('PAM_STREAM_QUEUE_CAPACITY', 16),
        'requestTimeoutMs' => (int) env('PAM_REQUEST_TIMEOUT_MS', 30_000),
        'maxConcurrentRequests' => 1,
        'phpExecutorQueueCapacity' => (int) env('PAM_PHP_QUEUE_CAPACITY', 1_024),
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
    ],
];
