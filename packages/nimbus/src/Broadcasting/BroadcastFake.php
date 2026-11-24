<?php

declare(strict_types=1);

namespace Nimbus\Broadcasting;

/** Records broadcasts instead of sending them, with assertions for tests. */
final class BroadcastFake implements Broadcaster
{
    /** @var list<array{channels: non-empty-list<string>, event: string, payload: array<array-key, mixed>, except: ?string}> */
    private array $broadcasts = [];

    public function broadcast(array $channels, string $event, array $payload, ?string $exceptSocketId = null): void
    {
        $this->broadcasts[] = ['channels' => $channels, 'event' => $event, 'payload' => $payload, 'except' => $exceptSocketId];
    }

    public function authorizeSocket(string $channel, string $socketId, ?array $presence = null): array
    {
        return $presence === null ? [] : ['channel_data' => $presence];
    }

    /**
     * The event (its broadcast name or class) was broadcast, optionally on the channel and
     * matching the payload callback.
     *
     * @param (\Closure(array<array-key, mixed>): bool)|null $callback
     */
    public function assertBroadcast(string $event, ?string $channel = null, ?\Closure $callback = null): void
    {
        self::assert($this->broadcasts($event, $channel, $callback) !== [], "The expected [{$event}] broadcast was not sent.");
    }

    public function assertNotBroadcast(string $event, ?string $channel = null): void
    {
        self::assert($this->broadcasts($event, $channel) === [], "The unexpected [{$event}] broadcast was sent.");
    }

    public function assertNothingBroadcast(): void
    {
        self::assert($this->broadcasts === [], count($this->broadcasts) . ' unexpected broadcasts were sent.');
    }

    /**
     * @param (\Closure(array<array-key, mixed>): bool)|null $callback
     * @return list<array{channels: non-empty-list<string>, event: string, payload: array<array-key, mixed>, except: ?string}>
     */
    public function broadcasts(?string $event = null, ?string $channel = null, ?\Closure $callback = null): array
    {
        return array_values(array_filter(
            $this->broadcasts,
            static fn (array $broadcast): bool => ($event === null || $broadcast['event'] === $event)
                && ($channel === null || in_array($channel, $broadcast['channels'], true))
                && ($callback === null || $callback($broadcast['payload']) === true),
        ));
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
