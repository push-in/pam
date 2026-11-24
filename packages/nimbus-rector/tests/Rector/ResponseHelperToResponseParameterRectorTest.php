<?php

declare(strict_types=1);

namespace Nimbus\Rector\Tests\Rector;

final class ResponseHelperToResponseParameterRectorTest extends RuleTestCase
{
    protected static function fixtureDirectory(): string
    {
        return 'response';
    }

    public function provideConfigFilePath(): string
    {
        return __DIR__ . '/config/response.php';
    }
}
