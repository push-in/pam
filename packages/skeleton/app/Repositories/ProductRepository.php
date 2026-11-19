<?php

declare(strict_types=1);

namespace App\Repositories;

use App\Data\CreateProductData;
use App\Models\Product;

interface ProductRepository
{
    /** @return iterable<Product> */
    public function all(): iterable;

    public function create(CreateProductData $data): Product;
}
