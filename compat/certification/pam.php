<?php

declare(strict_types=1);

use Pam\Laravel\Application;

Application::boot(__DIR__)->listen(
    port: (int) (getenv('PAM_CERTIFICATION_PORT') ?: 31400),
    options: [
        'maxConcurrentRequests' => 1,
        'maxResponseBytes' => 8 * 1024 * 1024,
    ],
);
