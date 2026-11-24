<?php

declare(strict_types=1);

namespace Nimbus\Rector\Tests\Rector;

final class LaravelToNimbusSetTest extends RuleTestCase
{
    protected static function fixtureDirectory(): string
    {
        return 'set';
    }

    public function provideConfigFilePath(): string
    {
        return __DIR__ . '/config/set.php';
    }
}
