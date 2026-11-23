<?php

declare(strict_types=1);

namespace Nimbus\Testing;

use Nimbus\Events\EventDispatcher;
use Nimbus\Events\EventFake;

/**
 * Test facade: `Event::fake()` binds an EventFake as the application's EventDispatcher;
 * with `$only`, other events still reach the original dispatcher.
 */
final class Event
{
    private function __construct()
    {
    }

    /** @param list<class-string> $only */
    public static function fake(array $only = []): EventFake
    {
        $container = Fakes::app()->container();
        $inner = $container->has(EventDispatcher::class) ? $container->get(EventDispatcher::class) : null;
        $fake = new EventFake($inner instanceof EventDispatcher ? $inner : null, $only);
        $container->instance(EventDispatcher::class, $fake);
        return Fakes::remember($fake);
    }

    /**
     * @template T of object
     * @param class-string<T> $event
     * @param (\Closure(T): bool)|int|null $callback
     */
    public static function assertDispatched(string $event, \Closure|int|null $callback = null): void
    {
        self::faked()->assertDispatched($event, $callback);
    }

    /** @param class-string $event */
    public static function assertDispatchedTimes(string $event, int $times): void
    {
        self::faked()->assertDispatchedTimes($event, $times);
    }

    /**
     * @template T of object
     * @param class-string<T> $event
     * @param (\Closure(T): bool)|null $callback
     */
    public static function assertNotDispatched(string $event, ?\Closure $callback = null): void
    {
        self::faked()->assertNotDispatched($event, $callback);
    }

    public static function assertNothingDispatched(): void
    {
        self::faked()->assertNothingDispatched();
    }

    /** @param class-string $event */
    public static function assertListening(string $event, string $listener): void
    {
        self::faked()->assertListening($event, $listener);
    }

    private static function faked(): EventFake
    {
        return Fakes::get(EventFake::class, 'Event');
    }
}
