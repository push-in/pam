<?php

declare(strict_types=1);

namespace Nimbus\Tests\Events\Fixtures;


final readonly class OrderShipped
{
    /** @param list<string> $items */
    public function __construct(public int $orderId, public array $items = [], public ?string $note = null)
    {
    }
}
