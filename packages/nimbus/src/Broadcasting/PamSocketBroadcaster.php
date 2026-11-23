<?php

declare(strict_types=1);

namespace Nimbus\Broadcasting;

/**
 * Broadcasts through PAM's native WebSocket server: a channel is a room, and an event is
 * emitted to the room's sockets (across workers and nodes through the server's Redis
 * Streams or NATS adapter). Authorizing a private or presence channel joins the socket
 * (the PAM socket id the client sends as `socket_id`) to the room, so clients never
 * join private rooms themselves.
 */
final readonly class PamSocketBroadcaster implements Broadcaster
{
    /**
     * @param ?\Closure(string, string, array<array-key, mixed>): void $emit Emits to a room;
     *        defaults to `Pam\WS\Server::to($room)->emit()`.
     * @param ?\Closure(string, string): void $join Joins a socket to a room; defaults to
     *        `Pam\WS\Socket::join()`.
     */
    public function __construct(
        private ?\Closure $emit = null,
        private ?\Closure $join = null,
    ) {
        if (($emit === null || $join === null) && !class_exists(\Pam\WS\Server::class)) {
            throw new \LogicException('PamSocketBroadcaster requires the PAM runtime WebSocket server.');
        }
    }

    public function broadcast(array $channels, string $event, array $payload, ?string $exceptSocketId = null): void
    {
        foreach ($channels as $channel) {
            if ($this->emit !== null) {
                ($this->emit)($channel, $event, $payload);
                continue;
            }
            (new \Pam\WS\Server())->to($channel)->emit($event, $payload);
        }
    }

    public function authorizeSocket(string $channel, string $socketId, ?array $presence = null): array
    {
        if ($this->join !== null) {
            ($this->join)($socketId, $channel);
        } else {
            (new \Pam\WS\Socket($socketId))->join($channel);
        }
        if ($presence !== null) {
            $this->broadcast([$channel], 'presence:joined', $presence);
            return ['channel_data' => $presence];
        }
        return [];
    }
}
