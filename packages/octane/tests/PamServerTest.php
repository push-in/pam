<?php

declare(strict_types=1);

namespace Pam\Octane\Tests;

use Illuminate\Config\Repository;
use Pam\Octane\PamServer;
use PHPUnit\Framework\TestCase;

final class PamServerTest extends TestCase
{
    public function testTheRequestTimeoutFollowsOctaneMaxExecutionTime(): void
    {
        self::assertSame(30_000, self::timeout([]));
        self::assertSame(2_000, self::timeout(['max_execution_time' => 2]));
        self::assertSame(5_000, self::timeout(['max_execution_time' => '5']));
        self::assertSame(365 * 24 * 3600 * 1000, self::timeout(['max_execution_time' => 0]));
        self::assertSame(30_000, self::timeout(['max_execution_time' => -1]));
    }

    /** @param array<string, mixed> $octane */
    private static function timeout(array $octane): int
    {
        $method = new \ReflectionMethod(PamServer::class, 'requestTimeoutMs');

        $value = $method->invoke(null, new Repository(['octane' => $octane]));
        self::assertIsInt($value);

        return $value;
    }
}
