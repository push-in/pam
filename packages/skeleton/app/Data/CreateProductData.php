<?php

declare(strict_types=1);

namespace App\Data;

final readonly class CreateProductData
{
    public function __construct(
        public string $name,
        public int $priceInCents,
    ) {
    }
}
