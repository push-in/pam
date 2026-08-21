<?php

declare(strict_types=1);

namespace App\Http\Requests;

use App\Domain\Products\CreateProductData;
use Pam\Api\Validation\FormRequest;

final class StoreProductRequest extends FormRequest
{
    public function rules(): array
    {
        return [
            'name' => ['required', 'string'],
            'priceInCents' => ['required', 'integer'],
        ];
    }

    public function data(): CreateProductData
    {
        return $this->dto(CreateProductData::class);
    }
}
