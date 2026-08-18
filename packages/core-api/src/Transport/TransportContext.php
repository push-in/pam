<?php

declare(strict_types=1);

namespace Pam\Contracts\Transport;

final readonly class TransportContext
{
    public function __construct(
        public string $workerId,
        public \Closure $cancelled,
        public \Closure $observe,
    ) {
        if (preg_match('/^[A-Za-z0-9_.-]{1,96}$/D', $workerId) !== 1) {
            throw new \InvalidArgumentException('Transport worker ID is invalid.');
        }
    }

    public function isCancelled(): bool
    {
        return (bool) ($this->cancelled)();
    }

    /** @param array<string, int|float|string|bool|null> $attributes */
    public function record(TransportEventCode $event, array $attributes = []): void
    {
        ($this->observe)($event->value, $attributes);
    }
}
