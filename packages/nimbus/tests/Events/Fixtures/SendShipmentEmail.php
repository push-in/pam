<?php

declare(strict_types=1);

namespace Nimbus\Tests\Events\Fixtures;

use Nimbus\Events\ShouldQueue;

final class SendShipmentEmail implements ShouldQueue
{
    public int $tries = 5;

    public int $delay = 30;

    /** @var list<int> */
    public static array $handled = [];

    public function __invoke(OrderShipped $event): void
    {
        self::$handled[] = $event->orderId;
    }
}
