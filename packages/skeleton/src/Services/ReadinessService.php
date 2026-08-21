<?php

declare(strict_types=1);

namespace App\Services;

final readonly class ReadinessService
{
    public function snapshot(): ReadinessSnapshot
    {
        $requestId = $_SERVER['PAM_REQUEST_ID'] ?? null;
        return new ReadinessSnapshot(
            ReadinessStatus::Ready,
            'pong',
            is_string($requestId) ? $requestId : null,
        );
    }
}
