<?php

declare(strict_types=1);

namespace App\Services;

final readonly class ReadinessSnapshot
{
    public function __construct(
        public ReadinessStatus $status,
        public string $message,
        public ?string $requestId,
    ) {
    }
}
