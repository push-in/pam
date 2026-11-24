<?php

declare(strict_types=1);

namespace Nimbus\Tests;

use Nimbus\Deprecation;
use PHPUnit\Framework\TestCase;

final class DeprecationTest extends TestCase
{
    public function testEmitsAUserDeprecationInTheSymfonyContractFormat(): void
    {
        $notices = [];
        set_error_handler(static function (int $level, string $message) use (&$notices): bool {
            $notices[] = [$level, $message];
            return true;
        }, E_USER_DEPRECATED);
        try {
            Deprecation::trigger('1.2', 'Passing a string to %s() is deprecated, pass a %s instead.', 'App::middleware', 'MiddlewareInterface');
        } finally {
            restore_error_handler();
        }

        self::assertSame([[
            E_USER_DEPRECATED,
            'Since pushinbr/nimbus 1.2: Passing a string to App::middleware() is deprecated, pass a MiddlewareInterface instead.',
        ]], $notices);
    }
}
