<?php

declare(strict_types=1);

namespace Nimbus\Broadcasting;

use Nimbus\Redis\Connection;

/**
 * Publishes `{"event", "data", "socket"}` messages on Redis pub/sub channels (the format
 * Laravel Echo Server and Soketi's Redis adapter read). Through the PAM Redis client,
 * publishing suspends only the request Fiber.
 */
final readonly class RedisBroadcaster implements Broadcaster
{
    public function __construct(
        private Connection $redis,
        private string $prefix = '',
    ) {
    }

    public function broadcast(array $channels, string $event, array $payload, ?string $exceptSocketId = null): void
    {
        $message = json_encode(
            ['event' => $event, 'data' => $payload, 'socket' => $exceptSocketId],
            JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE,
        );
        foreach ($channels as $channel) {
            $this->redis->command(['PUBLISH', $this->prefix . $channel, $message]);
        }
    }

    public function authorizeSocket(string $channel, string $socketId, ?array $presence = null): array
    {
        return $presence === null ? [] : ['channel_data' => $presence];
    }
}
