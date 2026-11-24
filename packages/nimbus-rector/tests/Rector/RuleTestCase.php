<?php

declare(strict_types=1);

namespace Nimbus\Rector\Tests\Rector;

use Rector\Testing\PHPUnit\AbstractRectorTestCase;

/** Runs every `Fixture/<name>/*.php.inc` (input, `-----`, expected output) through one config. */
abstract class RuleTestCase extends AbstractRectorTestCase
{
    abstract protected static function fixtureDirectory(): string;

    /** @return \Iterator<array-key, array{string}> */
    public static function provideFixtures(): \Iterator
    {
        return self::yieldFilesFromDirectory(__DIR__ . '/Fixture/' . static::fixtureDirectory());
    }

    #[\PHPUnit\Framework\Attributes\DataProvider('provideFixtures')]
    public function testFixture(string $filePath): void
    {
        $this->doTestFile($filePath);
    }
}
