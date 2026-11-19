<?php

declare(strict_types=1);

namespace Nimbus\Notifications;

/**
 * The channel registry: renders notifications per channel and delivers payloads.
 */
final class NotificationSender
{
    /** @var array<string, NotificationChannel> */
    private array $channels = [];

    /** @param array<string, NotificationChannel> $channels */
    public function __construct(array $channels = [])
    {
        foreach ($channels as $name => $channel) {
            $this->channel($name, $channel);
        }
    }

    public function channel(string $name, NotificationChannel $channel): self
    {
        if (preg_match('/^[a-z][a-z0-9_.-]{0,63}$/D', $name) !== 1) {
            throw new \InvalidArgumentException('Channel names must be short lowercase identifiers.');
        }
        $this->channels[$name] = $channel;
        return $this;
    }

    public function has(string $name): bool
    {
        return isset($this->channels[$name]);
    }

    public function get(string $name): NotificationChannel
    {
        return $this->channels[$name] ?? throw new \InvalidArgumentException("Unknown notification channel: {$name}.");
    }

    /**
     * Renders the notification on every channel of `via()`; channels without a route are skipped.
     *
     * @return list<array{channel: string, payload: array<string, mixed>}>
     */
    public function prepare(Notifiable $notifiable, Notification $notification, ?string $id = null): array
    {
        $id ??= self::uuid();
        $prepared = [];
        foreach (array_values(array_unique($notification->via($notifiable))) as $name) {
            $payload = $this->get($name)->prepare($notifiable, $notification, $id);
            if ($payload !== null) {
                $prepared[] = ['channel' => $name, 'payload' => $payload];
            }
        }
        return $prepared;
    }

    /** @param array<string, mixed> $payload */
    public function deliver(string $channel, array $payload): void
    {
        $this->get($channel)->deliver($payload);
    }

    public function send(Notifiable $notifiable, Notification $notification): void
    {
        foreach ($this->prepare($notifiable, $notification) as $item) {
            $this->deliver($item['channel'], $item['payload']);
        }
    }

    /** @return non-empty-string */
    public static function uuid(): string
    {
        $bytes = random_bytes(16);
        $bytes[6] = chr((ord($bytes[6]) & 0x0f) | 0x40);
        $bytes[8] = chr((ord($bytes[8]) & 0x3f) | 0x80);
        return vsprintf('%s%s-%s-%s-%s-%s%s%s', str_split(bin2hex($bytes), 4));
    }
}
