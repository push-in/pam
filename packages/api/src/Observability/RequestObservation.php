<?php

declare(strict_types=1);

namespace Pam\Api\Observability;

final readonly class RequestObservation
{
    public function __construct(
        public string $method,
        public string $route,
        public int $status,
        public float $durationSeconds,
        public ?string $exceptionClass = null,
    ) {
    }
}

