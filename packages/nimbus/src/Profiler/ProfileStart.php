<?php

declare(strict_types=1);

namespace Nimbus\Profiler;

/** @internal */
final readonly class ProfileStart
{
    public function __construct(
        public string $token,
        public int $nanoseconds,
        public int $memoryBytes,
    ) {
    }
}
