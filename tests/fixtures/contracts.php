<?php

declare(strict_types=1);

use Pam\Contract\Data;
use Pam\Contract\Field;

#[Data(description: 'Lifecycle of an order.')]
enum OrderStatus: int
{
    case Pending = 1;
    case Paid = 2;
    case Shipped = 3;
}

#[Data(description: 'Postal destination.')]
final readonly class Address
{
    public function __construct(
        #[Field(description: 'Destination city.')]
        public string $city,
        #[Field(description: 'ISO 3166-1 alpha-2 country code.', format: 'iso-country')]
        public string $country,
    ) {
    }
}

#[Data(description: 'Command accepted by the order boundary.')]
final readonly class CreateOrder
{
    /**
     * @param list<string> $tags
     */
    public function __construct(
        #[Field(description: 'Stable command identifier.', format: 'uuid')]
        public string $id,
        public OrderStatus $status,
        #[Field(description: 'Searchable labels.', itemType: 'string')]
        public array $tags,
        public ?Address $shipping,
        #[Field(description: 'Requested units.', minimum: 1, maximum: 1000)]
        public int $quantity,
    ) {
    }
}
