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
    'telemetry' => [
        'enabled' => (bool) env('PAM_OTLP_ENABLED', false),
        'service_name' => env('OTEL_SERVICE_NAME', env('APP_NAME', 'laravel')),
        'service_version' => env('OTEL_SERVICE_VERSION', env('APP_VERSION', 'unknown')),
        'buffer_limit' => (int) env('PAM_OTLP_BUFFER_LIMIT', 512),
        'fail_hard' => (bool) env('PAM_OTLP_FAIL_HARD', false),
        'otlp' => [
            'endpoint' => env('OTEL_EXPORTER_OTLP_TRACES_ENDPOINT', env('OTEL_EXPORTER_OTLP_ENDPOINT', 'http://127.0.0.1:4318')),
            'headers' => [],
            'header_string' => env('OTEL_EXPORTER_OTLP_TRACES_HEADERS', env('OTEL_EXPORTER_OTLP_HEADERS', '')),
            'timeout_ms' => (int) env('OTEL_EXPORTER_OTLP_TRACES_TIMEOUT', env('OTEL_EXPORTER_OTLP_TIMEOUT', 500)),
            'compression' => env('OTEL_EXPORTER_OTLP_TRACES_COMPRESSION', env('OTEL_EXPORTER_OTLP_COMPRESSION', 'none')),
        ],
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
        'required_extensions' => ['ctype', 'filter', 'json', 'mbstring', 'openssl', 'pcntl', 'pdo', 'posix', 'tokenizer'],
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
        'stop_timeout_seconds' => (int) env('PAM_PROCESS_STOP_TIMEOUT', 10),
    ],
    'autoscaling' => [
        'min_instances' => (int) env('PAM_AUTOSCALE_MIN', 1),
        'max_instances' => (int) env('PAM_AUTOSCALE_MAX', 16),
        'target_cpu_percent' => (float) env('PAM_AUTOSCALE_TARGET_CPU', 65),
        'target_p95_ms' => (float) env('PAM_AUTOSCALE_TARGET_P95_MS', 250),
        'cooldown_seconds' => (int) env('PAM_AUTOSCALE_COOLDOWN', 60),
        'metrics_url' => env('PAM_AUTOSCALE_METRICS_URL'),
        'metrics_token' => env('PAM_AUTOSCALE_METRICS_TOKEN'),
    ],
    'nightwatch' => [
        'token' => env('NIGHTWATCH_TOKEN'),
    ],
    'remote' => [
        'timeout_seconds' => (int) env('PAM_REMOTE_TIMEOUT', 15),
        'allow_insecure_http' => (bool) env('PAM_REMOTE_ALLOW_HTTP', false),
        'cloud' => [
            'url' => env('PAM_CLOUD_URL'),
            'project' => env('PAM_CLOUD_PROJECT'),
            'token' => env('PAM_CLOUD_TOKEN'),
        ],
        'targets' => [
            'production' => [
                'provider' => (int) env('PAM_REMOTE_PRODUCTION_PROVIDER', 1),
                'webhook' => env('PAM_FORGE_PRODUCTION_WEBHOOK'),
            ],
            'staging' => [
                'provider' => (int) env('PAM_REMOTE_STAGING_PROVIDER', 1),
                'webhook' => env('PAM_FORGE_STAGING_WEBHOOK'),
            ],
        ],
    ],
    'mcp' => [
        'allow_mutations' => (bool) env('PAM_MCP_ALLOW_MUTATIONS', false),
        'confirmation_token' => env('PAM_MCP_CONFIRMATION_TOKEN'),
    ],
    'compatibility_registry' => env(
        'PAM_COMPATIBILITY_REGISTRY',
        'https://raw.githubusercontent.com/push-in/pam/main/compatibility/laravel-packages.json',
    ),
];
