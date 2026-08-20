<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Container;

use Pam\Api\Container\Container;
use PHPUnit\Framework\Attributes\CoversClass;
use PHPUnit\Framework\TestCase;

#[CoversClass(Container::class)]
final class ContainerTest extends TestCase
{
    public function testItAutowiresConstructorDependencies(): void
    {
        $container = new Container();

        self::assertInstanceOf(DependentService::class, $container->get(DependentService::class));
    }

    public function testScopedBindingsAreReusedAndThenDiscarded(): void
    {
        $container = new Container();
        $container->scoped(ScopedValue::class);

        $container->beginScope();
        $first = $container->get(ScopedValue::class);
        self::assertSame($first, $container->get(ScopedValue::class));
        $container->endScope();

        $container->beginScope();
        self::assertNotSame($first, $container->get(ScopedValue::class));
        $container->endScope();
    }

    public function testScopedBindingsCannotResolveOutsideARequest(): void
    {
        $container = new Container();
        $container->scoped(ScopedValue::class);

        $this->expectException(\LogicException::class);
        $container->get(ScopedValue::class);
    }

    public function testConcurrentFibersKeepIndependentRequestScopes(): void
    {
        $container = new Container();
        $container->scoped(ScopedValue::class);

        $request = static function () use ($container): void {
            $container->beginScope();
            $value = $container->get(ScopedValue::class);
            \Fiber::suspend($value);
            self::assertSame($value, $container->get(ScopedValue::class));
            $container->endScope();
        };

        $first = new \Fiber($request);
        $second = new \Fiber($request);
        $firstValue = $first->start();
        $secondValue = $second->start();

        self::assertInstanceOf(ScopedValue::class, $firstValue);
        self::assertInstanceOf(ScopedValue::class, $secondValue);
        self::assertNotSame($firstValue, $secondValue);

        $first->resume();
        $second->resume();
    }
}

final class Dependency
{
}

final readonly class DependentService
{
    public function __construct(public Dependency $dependency)
    {
    }
}

final class ScopedValue
{
}
