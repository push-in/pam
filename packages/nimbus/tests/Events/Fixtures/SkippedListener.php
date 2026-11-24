<?php

declare(strict_types=1);

namespace Nimbus\Tests\Events\Fixtures;

use Nimbus\Events\ShouldQueue;

final class SkippedListener implements ShouldQueue
{
    public function shouldQueue(OrderShipped $event): bool
    {
        return $event->orderId !== 13;
    }

    public function __invoke(OrderShipped $event): void
    {
    }
}
