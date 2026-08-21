<?php

declare(strict_types=1);

namespace App\Services;

final readonly class ReadinessService
{
    public function snapshot(?string $requestId): ReadinessSnapshot
    {
        return new ReadinessSnapshot(
            ReadinessStatus::Ready,
            'pong',
            $requestId,
        );
    }
}
