<?php

declare(strict_types=1);

namespace Nimbus\Broadcasting;

use Pusher\Pusher;

/**
 * The Pusher protocol through pusher/pusher-php-server: Pusher Channels, or a compatible
 * server such as Soketi or Laravel Reverb (set host, port and scheme), with pusher-js or
 * Laravel Echo clients.
 */
final readonly class PusherBroadcaster implements Broadcaster
{
    public function __construct(private Pusher $pusher)
    {
    }

    /**
     * From PUSHER_APP_ID, PUSHER_APP_KEY, PUSHER_APP_SECRET and the optional PUSHER_HOST,
     * PUSHER_PORT, PUSHER_SCHEME (https) and PUSHER_APP_CLUSTER.
     *
     * @param array<string, string>|null $environment Defaults to getenv().
     */
    public static function fromEnvironment(?array $environment = null): self
    {
        if (!class_exists(Pusher::class)) {
            throw new \LogicException('PusherBroadcaster requires pusher/pusher-php-server: composer require pusher/pusher-php-server.');
        }
        $environment ??= getenv();
        foreach (['PUSHER_APP_ID', 'PUSHER_APP_KEY', 'PUSHER_APP_SECRET'] as $key) {
            if (($environment[$key] ?? '') === '') {
                throw new \RuntimeException("{$key} must be set.");
            }
        }
        $scheme = $environment['PUSHER_SCHEME'] ?? 'https';
        $options = ['useTLS' => $scheme === 'https', 'scheme' => $scheme];
        if (($environment['PUSHER_HOST'] ?? '') !== '') {
            $options['host'] = $environment['PUSHER_HOST'];
            $options['port'] = (int) ($environment['PUSHER_PORT'] ?? ($scheme === 'https' ? 443 : 80));
        } elseif (($environment['PUSHER_APP_CLUSTER'] ?? '') !== '') {
            $options['cluster'] = $environment['PUSHER_APP_CLUSTER'];
        }
        return new self(new Pusher(
            $environment['PUSHER_APP_KEY'],
            $environment['PUSHER_APP_SECRET'],
            $environment['PUSHER_APP_ID'],
            $options,
        ));
    }

    public function broadcast(array $channels, string $event, array $payload, ?string $exceptSocketId = null): void
    {
        $parameters = $exceptSocketId !== null ? ['socket_id' => $exceptSocketId] : [];
        // The Pusher HTTP API accepts at most 100 channels per trigger.
        foreach (array_chunk($channels, 100) as $chunk) {
            $this->pusher->trigger($chunk, $event, $payload, $parameters);
        }
    }

    public function authorizeSocket(string $channel, string $socketId, ?array $presence = null): array
    {
        $body = $presence === null
            ? $this->pusher->authorizeChannel($channel, $socketId)
            : $this->pusher->authorizePresenceChannel($channel, $socketId, $presence['user_id'], $presence['user_info']);
        $decoded = json_decode($body, true, 16, JSON_THROW_ON_ERROR);
        if (!is_array($decoded)) {
            throw new \UnexpectedValueException('The Pusher library returned an invalid auth response.');
        }
        /** @var array<string, mixed> $decoded */
        return $decoded;
    }
}
