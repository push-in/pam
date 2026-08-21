<?php

declare(strict_types=1);

namespace App\Repositories;

use App\Domain\Products\CreateProductData;
use App\Domain\Products\ProductStatus;
use App\Models\Product;

final readonly class EloquentProductRepository implements ProductRepository
{
    public function all(): iterable
    {
        $model = new Product();
        $products = [];
        foreach ($model->getConnection()->table($model->getTable())->orderBy('id')->get() as $row) {
            $products[] = $model->newFromBuilder((array) $row);
        }
        return $products;
    }

    public function create(CreateProductData $data): Product
    {
        return Product::query()->create([
            'name' => $data->name,
            'price_in_cents' => $data->priceInCents,
            'status' => ProductStatus::Active,
        ]);
    }
}
