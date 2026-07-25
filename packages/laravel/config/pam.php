<?php

declare(strict_types=1);

return [
    'health' => [
        'enabled' => (bool) env('PAM_LARAVEL_HEALTH_ENABLED', true),
        'path' => env('PAM_LARAVEL_HEALTH_PATH', '/__pam/health'),
        'metrics_path' => env('PAM_LARAVEL_METRICS_PATH', '/__pam/metrics'),
        'token' => env('PAM_LARAVEL_OBSERVABILITY_TOKEN'),
    ],
    'observability' => [
        'enabled' => (bool) env('PAM_LARAVEL_OBSERVABILITY', true),
        'slow_request_ms' => (int) env('PAM_LARAVEL_SLOW_REQUEST_MS', 500),
        'slow_query_ms' => (int) env('PAM_LARAVEL_SLOW_QUERY_MS', 100),
        'route_limit' => (int) env('PAM_LARAVEL_ROUTE_LIMIT', 256),
        'query_limit' => (int) env('PAM_LARAVEL_QUERY_LIMIT', 128),
        'n_plus_one_threshold' => (int) env('PAM_LARAVEL_N_PLUS_ONE_THRESHOLD', 8),
        'response_headers' => (bool) env('PAM_LARAVEL_TIMING_HEADERS', false),
    ],
    'state_guard' => [
        'enabled' => (bool) env('PAM_LARAVEL_STATE_GUARD', true),
        'strict' => (bool) env('PAM_LARAVEL_STATE_GUARD_STRICT', false),
    ],
    'production' => [
        'require_config_cache' => (bool) env('PAM_REQUIRE_CONFIG_CACHE', true),
        'require_route_cache' => (bool) env('PAM_REQUIRE_ROUTE_CACHE', false),
        'distributed_cache' => (bool) env('PAM_REQUIRE_DISTRIBUTED_CACHE', true),
        'distributed_session' => (bool) env('PAM_REQUIRE_DISTRIBUTED_SESSION', true),
        'queue_sync_allowed' => (bool) env('PAM_QUEUE_SYNC_ALLOWED', false),
        'required_extensions' => ['ctype', 'filter', 'json', 'mbstring', 'openssl', 'pdo', 'tokenizer'],
    ],
    'deploy' => [
        'root' => env('PAM_DEPLOY_ROOT', base_path('releases')),
        'current' => env('PAM_DEPLOY_CURRENT', base_path('current')),
        'keep_releases' => (int) env('PAM_DEPLOY_KEEP_RELEASES', 5),
        'readiness_url' => env('PAM_DEPLOY_READINESS_URL', 'http://127.0.0.1:3010/ready'),
        'readiness_timeout_seconds' => (int) env('PAM_DEPLOY_READINESS_TIMEOUT', 30),
        'migrate' => (bool) env('PAM_DEPLOY_MIGRATE', true),
        'binary' => env('PAM_DEPLOY_BINARY', 'pam'),
    ],
    'supervisor' => [
        'manifest' => env('PAM_PROCESS_MANIFEST', base_path('pam.processes.json')),
        'state_path' => env('PAM_PROCESS_STATE', storage_path('pam/processes')),
        'log_path' => env('PAM_PROCESS_LOGS', storage_path('logs/pam')),
    ],
    'compatibility_registry' => env(
        'PAM_COMPATIBILITY_REGISTRY',
        'https://raw.githubusercontent.com/push-in/pam/main/compatibility/laravel-packages.json',
    ),
];
