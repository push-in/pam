<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Webhook;

/**
 * Body and extra headers of a webhook delivery.
 */
final class WebhookMessage
{
    /** @var array<string, string> */
    private array $headers = [];
    private ?string $event = null;

    /** @param array<string, mixed> $data */
    public function __construct(private array $data = [])
    {
    }

    /** @param array<string, mixed> $data */
    public static function make(array $data = []): self
    {
        return new self($data);
    }

    /** @param array<string, mixed> $data */
    public function data(array $data): self
    {
        $this->data = $data;
        return $this;
    }

    /** Overrides the event name (defaults to `Notification::type()`). */
    public function event(string $event): self
    {
        $this->event = $event;
        return $this;
    }

    public function header(string $name, string $value): self
    {
        if (preg_match('/^[A-Za-z0-9-]{1,64}$/D', $name) !== 1 || preg_match('/[\r\n\0]/', $value) === 1) {
            throw new \InvalidArgumentException('Webhook header is invalid.');
        }
        $this->headers[$name] = $value;
        return $this;
    }

    /** @return array{data: array<string, mixed>, event: ?string, headers: array<string, string>} */
    public function toArray(): array
    {
        return ['data' => $this->data, 'event' => $this->event, 'headers' => $this->headers];
    }
}
