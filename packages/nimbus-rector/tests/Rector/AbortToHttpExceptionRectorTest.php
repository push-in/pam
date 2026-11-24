<?php

declare(strict_types=1);

namespace Nimbus\Rector\Tests\Rector;

final class AbortToHttpExceptionRectorTest extends RuleTestCase
{
    protected static function fixtureDirectory(): string
    {
        return 'abort';
    }

    public function provideConfigFilePath(): string
    {
        return __DIR__ . '/config/abort.php';
    }
}
