<?php

declare(strict_types=1);

namespace Pam\Api\Routing;

interface RouteBindable
{
    public static function resolveRouteBinding(string $value): static;
}

