<?php

declare(strict_types=1);

namespace Pam\Compatibility\Tests;

use Pam\Compatibility\Probe;
use PHPUnit\Framework\TestCase;

final class ProbeTest extends TestCase
{
    public function testComposerPackagesAreLoadable(): void
    {
        $packages = Probe::packages();

        self::assertSame(42, $packages['amp']);
        foreach ($packages as $package) {
            self::assertNotFalse($package);
        }
    }
}
