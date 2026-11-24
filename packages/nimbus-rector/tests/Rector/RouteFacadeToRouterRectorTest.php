<?php

declare(strict_types=1);

namespace Nimbus\Rector\Tests\Rector;

final class RouteFacadeToRouterRectorTest extends RuleTestCase
{
    protected static function fixtureDirectory(): string
    {
        return 'route';
    }

    public function provideConfigFilePath(): string
    {
        return __DIR__ . '/config/route.php';
    }
}
