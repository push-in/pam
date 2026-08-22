<?php

declare(strict_types=1);

namespace Pam\Api\Health;

final readonly class HealthResult
{
    /** @param array<string, scalar|null> $details */
    public function __construct(
        public HealthState $state,
        public array $details = [],
    ) {
    }
}

