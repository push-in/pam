<?php

declare(strict_types=1);

namespace App\Http\Resources;

use App\Models\Product;
use Pam\Api\Http\JsonResource;
use Pam\Http\Request;

final readonly class ProductResource extends JsonResource
{
    public function toArray(Request $request): array
    {
        if (!$this->resource instanceof Product) {
            throw new \LogicException('ProductResource requires a Product.');
        }

        return [
            'id' => $this->resource->identifier(),
            'name' => $this->resource->name(),
            'priceInCents' => $this->resource->priceInCents(),
            'status' => $this->resource->status()->value,
        ];
    }
}
