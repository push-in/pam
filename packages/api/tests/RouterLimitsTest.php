<?php

declare(strict_types=1);

namespace Pam\Api\Tests;

use Pam\Api\Router;
use Pam\Api\RoutingResultType;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

final class RouterLimitsTest extends TestCase
{
    /** @return iterable<string, array{string, string}> */
    public static function invalidRoutes(): iterable
    {
        yield 'oversized method' => [str_repeat('A', 33), '/ok'];
        yield 'oversized path' => ['GET', '/' . str_repeat('a', 2_048)];
        yield 'too many segments' => ['GET', '/' . implode('/', array_fill(0, 129, 'a'))];
        yield 'too many parameters' => [
            'GET',
            '/' . implode('/', array_map(static fn (int $index): string => "{p{$index}}", range(1, 33))),
        ];
    }

    #[DataProvider('invalidRoutes')]
    public function testConfigurationLimitsFailBeforeRoutesArePublished(string $method, string $path): void
    {
        $router = new Router();

        $this->expectException(\InvalidArgumentException::class);
        $router->add($method, $path, static fn (): null => null);
    }

    public function testConfiguredRouteCountIsBounded(): void
    {
        $router = new Router(maximumRoutes: 1);
        $router->add('GET', '/one', static fn (): null => null);

        $this->expectException(\OverflowException::class);
        $router->add('GET', '/two', static fn (): null => null);
    }

    public function testUntrustedOversizedRequestPathDoesNotReachPcre(): void
    {
        $router = new Router();
        $router->add('GET', '/users/{id}', static fn (): null => null);

        self::assertSame(
            RoutingResultType::NotFound,
            $router->match('GET', '/' . str_repeat('a', 2_048))->type,
        );
    }

    public function testConstraintAndRouteNamesAreBounded(): void
    {
        $router = new Router();
        $route = $router->register('GET', '/users/{id}', static fn (): null => null);

        try {
            $router->constrain($route, 'id', str_repeat('a', 513));
            self::fail('Oversized constraint was accepted.');
        } catch (\InvalidArgumentException) {
        }

        $this->expectException(\InvalidArgumentException::class);
        $router->name($route, str_repeat('a', 129));
    }
}
