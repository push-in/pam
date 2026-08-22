<?php

declare(strict_types=1);

namespace Pam\Api\Events;

use Pam\Api\Container\Container;

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

    public function dispatch(object $event): object
    {
        foreach ($this->listeners[$event::class] ?? [] as $listener) {
            $resolved = is_string($listener) ? $this->container->get($listener) : $listener;
            if (!is_callable($resolved)) {
                throw new \UnexpectedValueException('Event listeners must resolve to callables.');
            }
            $resolved($event);
        }
        return $event;
    }
}
