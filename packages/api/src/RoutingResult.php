<?php

declare(strict_types=1);

namespace Pam\Api;

final readonly class RoutingResult
{
    /**
     * @param array<string, string> $parameters
     * @param list<string> $allowedMethods
     */
    public function __construct(
        public RoutingResultType $type,
        public ?Route $route = null,
        public array $parameters = [],
        public array $allowedMethods = [],
    ) {
    }
}
