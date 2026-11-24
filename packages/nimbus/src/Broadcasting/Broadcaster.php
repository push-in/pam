<?php

declare(strict_types=1);

namespace Nimbus\Broadcasting;

/**
 * A broadcasting driver: PAM's native WebSocket rooms, Redis pub/sub (Laravel Echo
 * Server style) or the Pusher protocol (Pusher, Soketi, Reverb).
 */
interface Broadcaster
{
    /**
     * @param non-empty-list<string> $channels
     * @param array<array-key, mixed> $payload
     * @param ?string $exceptSocketId The sender's socket, which does not receive the event.
     */
    public function broadcast(array $channels, string $event, array $payload, ?string $exceptSocketId = null): void;

    /**
     * The body of the successful auth response for a private or presence channel.
     *
     * @param ?array{user_id: string, user_info: array<array-key, mixed>} $presence Member data of a presence channel.
     * @return array<string, mixed>
     */
    public function authorizeSocket(string $channel, string $socketId, ?array $presence = null): array;
}
