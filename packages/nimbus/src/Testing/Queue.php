<?php

declare(strict_types=1);

namespace Nimbus\Testing;

use Nimbus\Jobs\JobDispatcher;
use Nimbus\Jobs\JobEnvelope;
use Nimbus\Jobs\QueueFake;

/**
 * Test facade: `Queue::fake()` binds a QueueFake as the application's JobDispatcher.
 * Fake before the services that receive the dispatcher are resolved.
 */
final class Queue
{
    private function __construct()
    {
    }

    public static function fake(): QueueFake
    {
        $fake = new QueueFake();
        Fakes::app()->container()->instance(JobDispatcher::class, $fake);
        return Fakes::remember($fake);
    }

    /**
     * @template T of object
     * @param class-string<T> $job
     * @param (\Closure(T, JobEnvelope): bool)|int|null $callback
     */
    public static function assertPushed(string $job, \Closure|int|null $callback = null): void
    {
        self::faked()->assertPushed($job, $callback);
    }

    /**
     * @template T of object
     * @param class-string<T> $job
     * @param (\Closure(T, JobEnvelope): bool)|null $callback
     */
    public static function assertNotPushed(string $job, ?\Closure $callback = null): void
    {
        self::faked()->assertNotPushed($job, $callback);
    }

    /** @param class-string $job */
    public static function assertPushedTimes(string $job, int $times): void
    {
        self::faked()->assertPushedTimes($job, $times);
    }

    public static function assertNothingPushed(): void
    {
        self::faked()->assertNothingPushed();
    }

    public static function assertListenerQueued(string $listener, ?string $event = null): void
    {
        self::faked()->assertListenerQueued($listener, $event);
    }

    private static function faked(): QueueFake
    {
        return Fakes::get(QueueFake::class, 'Queue');
    }
}
