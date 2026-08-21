<?php

declare(strict_types=1);

namespace App\Services;

use App\Domain\Products\CreateProductData;
use App\Models\Product;
use App\Repositories\ProductRepository;

final readonly class ProductService
{
    public function __construct(private ProductRepository $products)
    {
    }

    /** @return iterable<Product> */
    public function list(): iterable
    {
        return $this->products->all();
    }

    public function create(CreateProductData $data): Product
    {
        return $this->products->create($data);
    }
}
