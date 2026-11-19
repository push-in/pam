<?php

declare(strict_types=1);

namespace Nimbus\Routing;

interface RouteBindable
{
    public static function resolveRouteBinding(string $value): static;
}

