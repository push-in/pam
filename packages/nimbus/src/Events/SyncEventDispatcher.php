<?php

declare(strict_types=1);

namespace Nimbus\Events;

use Nimbus\Broadcasting\BroadcastManager;
use Nimbus\Broadcasting\ShouldBroadcast;
use Nimbus\Container\Container;
use Nimbus\Jobs\JobDispatcher;

/**
 * Calls the listeners of an event in registration order. Listener classes that implement
 * `ShouldQueue` are pushed to the bound `Jobs\JobDispatcher` instead, and events that
 * implement `Broadcasting\ShouldBroadcast` are handed to the bound `BroadcastManager`.
 */
final class SyncEventDispatcher implements EventDispatcher
{
    /** @var array<class-string, list<callable|string>> */
    private array $listeners = [];

    public function __construct(private readonly Container $container)
    {
    }

    /** @param class-string $event @param callable|class-string $listener */
    public function listen(string $event, callable|string $listener): self
    {
        $this->listeners[$event][] = $listener;
        return $this;
    }

    /**
     * @param class-string $event
     * @return list<callable|string>
     */
    public function listeners(string $event): array
    {
        return $this->listeners[$event] ?? [];
    }

    /** @param class-string $event */
    public function hasListener(string $event, string $listener): bool
    {
        return in_array($listener, $this->listeners[$event] ?? [], true);
    }

    public function dispatch(object $event): object
    {
        foreach ($this->listeners[$event::class] ?? [] as $listener) {
            if (is_string($listener) && is_subclass_of($listener, ShouldQueue::class)) {
                $this->queue($listener, $event);
                continue;
            }
            $resolved = is_string($listener) ? $this->container->get($listener) : $listener;
            if (!is_callable($resolved)) {
                throw new \UnexpectedValueException('Event listeners must resolve to callables.');
            }
            $resolved($event);
        }
        if ($event instanceof ShouldBroadcast && $this->container->has(BroadcastManager::class)) {
            $broadcasts = $this->container->get(BroadcastManager::class);
            if ($broadcasts instanceof BroadcastManager) {
                $broadcasts->dispatch($event, $this->jobs());
            }
        }
        return $event;
    }

    /** @param class-string<ShouldQueue> $listener */
    private function queue(string $listener, object $event): void
    {
        $jobs = $this->jobs() ?? throw new \LogicException("{$listener} is queued: bind a Nimbus\\Jobs\\JobDispatcher.");
        $instance = $this->container->get($listener);
        if (!is_object($instance)) {
            throw new \UnexpectedValueException("{$listener} did not resolve to a listener.");
        }
        if (method_exists($instance, 'shouldQueue') && $instance->shouldQueue($event) !== true) {
            return;
        }
        $tries = property_exists($instance, 'tries') && is_int($instance->tries) ? $instance->tries : 3;
        $delay = property_exists($instance, 'delay') && is_int($instance->delay) ? $instance->delay : 0;
        $jobs->dispatch(new CallQueuedListener($listener, $event::class, EventPayload::encode($event)), $tries, $delay);
    }

    private function jobs(): ?JobDispatcher
    {
        if (!$this->container->has(JobDispatcher::class)) {
            return null;
        }
        $jobs = $this->container->get(JobDispatcher::class);
        return $jobs instanceof JobDispatcher ? $jobs : null;
    }
}
