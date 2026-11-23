<?php

declare(strict_types=1);

namespace Nimbus\Tests\Events\Fixtures;


final class AuditOrder
{
    /** @var list<int> */
    public static array $seen = [];

    public function __invoke(OrderShipped $event): void
    {
        self::$seen[] = $event->orderId;
    }
}
