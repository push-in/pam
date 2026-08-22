<?php

declare(strict_types=1);

namespace Pam\Api\Profiler;

final readonly class RequestProfile
{
    public function __construct(
        public string $token,
        public string $method,
        public string $path,
        public int $statusCode,
        public float $durationMilliseconds,
        public int $memoryDeltaBytes,
        public ?string $failureClass,
    ) {
    }
}
