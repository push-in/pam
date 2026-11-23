<?php

declare(strict_types=1);

namespace Nimbus\Tests\Events\Fixtures;

use Nimbus\Broadcasting\PrivateChannel;
use Nimbus\Broadcasting\ShouldBroadcast;

final readonly class OrderStatusChanged implements ShouldBroadcast
{
    public function __construct(public int $orderId, public string $status, public ?string $socket = null)
    {
    }

    public function broadcastOn(): array
    {
        return [new PrivateChannel('orders.' . $this->orderId), 'dashboard'];
    }

    public function broadcastAs(): string
    {
        return 'order.status';
    }
}
