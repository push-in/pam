<?php

declare(strict_types=1);

namespace Nimbus\Events;

/**
 * Records dispatched events instead of running their listeners, with assertions for
 * tests. With `$only`, other events still reach the wrapped dispatcher.
 */
final class EventFake implements EventDispatcher
{
    /** @var list<object> */
    private array $dispatched = [];

    /** @param list<class-string> $only Events to fake; empty fakes all of them. */
    public function __construct(
        private readonly ?EventDispatcher $inner = null,
        private readonly array $only = [],
    ) {
    }

    public function inner(): ?EventDispatcher
    {
        return $this->inner;
    }

    public function dispatch(object $event): object
    {
        if ($this->only !== [] && !in_array($event::class, $this->only, true) && $this->inner !== null) {
            return $this->inner->dispatch($event);
        }
        $this->dispatched[] = $event;
        return $event;
    }

    /**
     * The event was dispatched: at least once, `$times` times when an int is given, or at
     * least once matching the callback.
     *
     * @template T of object
     * @param class-string<T> $event
     * @param (\Closure(T): bool)|int|null $callback
     */
    public function assertDispatched(string $event, \Closure|int|null $callback = null): void
    {
        if (is_int($callback)) {
            $this->assertDispatchedTimes($event, $callback);
            return;
        }
        self::assert($this->dispatched($event, $callback) !== [], "The expected [{$event}] event was not dispatched.");
    }

    /** @param class-string $event */
    public function assertDispatchedTimes(string $event, int $times): void
    {
        $count = count($this->dispatched($event));
        self::assert($count === $times, "The [{$event}] event was dispatched {$count} times instead of {$times}.");
    }

    /**
     * @template T of object
     * @param class-string<T> $event
     * @param (\Closure(T): bool)|null $callback
     */
    public function assertNotDispatched(string $event, ?\Closure $callback = null): void
    {
        self::assert($this->dispatched($event, $callback) === [], "The unexpected [{$event}] event was dispatched.");
    }

    public function assertNothingDispatched(): void
    {
        self::assert($this->dispatched === [], count($this->dispatched) . ' unexpected events were dispatched.');
    }

    /**
     * The wrapped SyncEventDispatcher has the listener registered for the event.
     *
     * @param class-string $event
     */
    public function assertListening(string $event, string $listener): void
    {
        self::assert(
            $this->inner instanceof SyncEventDispatcher && $this->inner->hasListener($event, $listener),
            "{$listener} does not listen to [{$event}].",
        );
    }

    /**
     * @template T of object
     * @param class-string<T> $event
     * @param (\Closure(T): bool)|null $callback
     * @return list<T>
     */
    public function dispatched(string $event, ?\Closure $callback = null): array
    {
        $matches = [];
        foreach ($this->dispatched as $dispatched) {
            if ($dispatched instanceof $event && ($callback === null || $callback($dispatched) === true)) {
                $matches[] = $dispatched;
            }
        }
        return $matches;
    }

    private static function assert(bool $condition, string $message): void
    {
        if (class_exists(\PHPUnit\Framework\Assert::class)) {
            \PHPUnit\Framework\Assert::assertTrue($condition, $message);
            return;
        }
        if (!$condition) {
            throw new \LogicException($message);
        }
    }
}
