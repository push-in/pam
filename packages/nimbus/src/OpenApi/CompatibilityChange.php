<?php

declare(strict_types=1);

namespace Nimbus\OpenApi;

final readonly class CompatibilityChange
{
    public function __construct(
        public CompatibilityChangeCode $code,
        public string $location,
        public string $message,
    ) {
    }
}

