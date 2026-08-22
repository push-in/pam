<?php

declare(strict_types=1);

namespace App\Domain\Products;

final readonly class CreateProductData
{
    public function __construct(
        public string $name,
        public int $priceInCents,
    ) {
    }
}
