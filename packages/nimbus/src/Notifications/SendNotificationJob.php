<?php

declare(strict_types=1);

namespace Nimbus\Notifications;

use Nimbus\Jobs\SerializableJob;

/**
 * One queued delivery: a channel name and the payload its `prepare()` rendered.
 */
final readonly class SendNotificationJob implements SerializableJob
{
    /** @param array<string, mixed> $payload */
    public function __construct(
        public string $channel,
        public array $payload,
    ) {
        if ($channel === '') {
            throw new \InvalidArgumentException('The notification channel is required.');
        }
    }

    public static function jobName(): string
    {
        return 'nimbus.notification';
    }

    public function toJobPayload(): array
    {
        return ['channel' => $this->channel, 'payload' => $this->payload];
    }

    public static function fromJobPayload(array $payload): self
    {
        $channel = $payload['channel'] ?? null;
        $data = $payload['payload'] ?? null;
        if (!is_string($channel) || !is_array($data)) {
            throw new \UnexpectedValueException('Notification job payload is invalid.');
        }
        /** @var array<string, mixed> $data */
        return new self($channel, $data);
    }
}
