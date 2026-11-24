<?php

declare(strict_types=1);

namespace Nimbus\Rector\Tests\Rector;

final class LaravelRequestCallToNimbusRectorTest extends RuleTestCase
{
    protected static function fixtureDirectory(): string
    {
        return 'request';
    }

    public function provideConfigFilePath(): string
    {
        return __DIR__ . '/config/request.php';
    }
}
