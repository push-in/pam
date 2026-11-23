<?php

declare(strict_types=1);

namespace Nimbus\Broadcasting;

use Nimbus\Jobs\SerializableJob;

/**
 * A queued broadcast: channels, event name and payload computed when the event was
 * dispatched. Register it in the `JobCodec` and run it with `InvokableJobHandler`.
 */
final readonly class BroadcastEvent implements SerializableJob
{
    /**
     * @param non-empty-list<string> $channels
     * @param array<array-key, mixed> $payload
     */
    public function __construct(
        public array $channels,
        public string $event,
        public array $payload,
        public ?string $exceptSocketId = null,
    ) {
    }

    public static function jobName(): string
    {
        return 'nimbus.broadcast';
    }

    public function toJobPayload(): array
    {
        return [
            'channels' => $this->channels,
            'event' => $this->event,
            'payload' => $this->payload,
            'except' => $this->exceptSocketId,
        ];
    }

    public static function fromJobPayload(array $payload): self
    {
        $channels = $payload['channels'] ?? null;
        $event = $payload['event'] ?? null;
        $data = $payload['payload'] ?? null;
        $except = $payload['except'] ?? null;
        if (!is_array($channels) || $channels === [] || !array_is_list($channels) || !is_string($event)
            || !is_array($data) || ($except !== null && !is_string($except))
        ) {
            throw new \UnexpectedValueException('Broadcast job payload is invalid.');
        }
        $names = [];
        foreach ($channels as $channel) {
            $names[] = is_string($channel) ? $channel : throw new \UnexpectedValueException('Broadcast job payload is invalid.');
        }
        return new self($names, $event, $data, $except);
    }

    public function handle(BroadcastManager $broadcasts): void
    {
        $broadcasts->broadcaster()->broadcast($this->channels, $this->event, $this->payload, $this->exceptSocketId);
    }
}
