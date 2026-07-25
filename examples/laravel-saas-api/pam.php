<?php

declare(strict_types=1);

use Pam\Laravel\Application;

Application::boot(__DIR__)->listen(
    port: (int) (getenv('PORT') ?: 3000),
    options: [
        'maxConcurrentRequests' => 1,
        'maxResponseBytes' => 16 * 1024 * 1024,
    ],
);
