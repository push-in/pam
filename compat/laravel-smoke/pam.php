<?php

declare(strict_types=1);

use Pam\Laravel\Application;

$maxResponseBytes = (int) (getenv('PAM_LARAVEL_MAX_RESPONSE_BYTES') ?: 256 * 1024 * 1024);

Application::boot(__DIR__)->listen(
    port: (int) (getenv('PAM_LARAVEL_SMOKE_PORT') ?: 31310),
    options: [
        'exposeErrors' => true,
        'maxConcurrentRequests' => (int) (getenv('PAM_LARAVEL_MAX_CONCURRENT_REQUESTS') ?: 1),
        'responseStreamQueueCapacity' => 4,
        'maxResponseBytes' => $maxResponseBytes,
        'maxResponseChunkBytes' => min(1024 * 1024, $maxResponseBytes),
        'leakDetectionSampleRate' => 64,
        'leakThresholdBytes' => 8 * 1024 * 1024,
    ],
);
