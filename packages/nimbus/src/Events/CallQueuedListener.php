<?php

declare(strict_types=1);

namespace Nimbus\Events;

use Nimbus\Container\Container;
use Nimbus\Jobs\SerializableJob;

/**
 * The job of a `ShouldQueue` listener: the listener class, the event class and its data.
 * Register it in the application's `JobCodec` and run it with `InvokableJobHandler`
 * (with the container), like any job with a `handle()` method.
 */
final readonly class CallQueuedListener implements SerializableJob
{
    /**
     * @param class-string $listener
     * @param class-string $event
     * @param array<string, mixed> $data
     */
    public function __construct(
        public string $listener,
        public string $event,
        public array $data,
    ) {
        if (!is_subclass_of($listener, ShouldQueue::class)) {
            throw new \InvalidArgumentException("{$listener} does not implement " . ShouldQueue::class . '.');
        }
    }

    public static function jobName(): string
    {
        return 'nimbus.queued-listener';
    }

    public function toJobPayload(): array
    {
        /** @var array<mixed> $data */
        $data = $this->data;
        return ['listener' => $this->listener, 'event' => $this->event, 'data' => $data];
    }

    public static function fromJobPayload(array $payload): self
    {
        $listener = $payload['listener'] ?? null;
        $event = $payload['event'] ?? null;
        $data = $payload['data'] ?? null;
        if (!is_string($listener) || !is_string($event) || !is_array($data) || !class_exists($listener) || !class_exists($event)) {
            throw new \UnexpectedValueException('Queued listener job payload is invalid.');
        }
        /** @var array<string, mixed> $data */
        return new self($listener, $event, $data);
    }

    /** Rebuilds the event and calls the listener, if it is still registered for the event. */
    public function handle(Container $container): void
    {
        $dispatcher = $container->has(EventDispatcher::class) ? $container->get(EventDispatcher::class) : null;
        if ($dispatcher instanceof EventFake) {
            $dispatcher = $dispatcher->inner();
        }
        if ($dispatcher instanceof SyncEventDispatcher && !$dispatcher->hasListener($this->event, $this->listener)) {
            throw new \LogicException("{$this->listener} is not a listener of {$this->event}.");
        }
        $listener = $container->get($this->listener);
        if (!is_callable($listener)) {
            throw new \UnexpectedValueException('Event listeners must resolve to callables.');
        }
        $listener(EventPayload::decode($this->event, $this->data));
    }
}
