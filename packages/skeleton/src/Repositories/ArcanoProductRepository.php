<?php

declare(strict_types=1);

namespace App\Repositories;

use App\Domain\Products\CreateProductData;
use App\Models\Product;
use Pam\Api\Database\ArcanoManager;

final readonly class ArcanoProductRepository implements ProductRepository
{
    public function __construct(private ArcanoManager $arcano)
    {
    }

    public function all(): iterable
    {
        $products = [];
        foreach ($this->arcano->select(Product::class)->orderBy('id')->fetchAll() as $product) {
            if (!$product instanceof Product) {
                throw new \LogicException('The product role must map to App\\Models\\Product.');
            }
            $products[] = $product;
        }
        return $products;
    }

    public function create(CreateProductData $data): Product
    {
        $product = new Product($data->name, $data->priceInCents);
        $this->arcano->entityManager()->persist($product)->run();
        return $product;
    }
}
