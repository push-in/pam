<?php

declare(strict_types=1);

namespace Nimbus\Tests\Events\Fixtures;

use Nimbus\Broadcasting\ShouldBroadcastNow;

final readonly class PriceTicked implements ShouldBroadcastNow
{
    public function __construct(public float $price)
    {
    }

    public function broadcastOn(): string
    {
        return 'prices';
    }

    /** @return array<string, float> */
    public function broadcastWith(): array
    {
        return ['p' => $this->price];
    }

    public function broadcastWhen(): bool
    {
        return $this->price > 0;
    }
}
