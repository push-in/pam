<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use App\Http\Requests\StoreProductRequest;
use App\Http\Resources\ProductResource;
use App\Services\ProductService;
use Pam\Api\Http\ResourceCollection;

final readonly class ProductController
{
    public function __construct(private ProductService $products)
    {
    }

    public function index(): ResourceCollection
    {
        return ProductResource::collection($this->products->list());
    }

    public function store(StoreProductRequest $request): ProductResource
    {
        return new ProductResource($this->products->create($request->data()), 201);
    }
}
