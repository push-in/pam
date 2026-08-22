<?php

declare(strict_types=1);

namespace App\Models;

use App\Domain\Products\ProductStatus;
use Illuminate\Database\Eloquent\Model;

final class Product extends Model
{
    /** @var list<string> */
    protected $fillable = ['name', 'price_in_cents', 'status'];

    /** @return array<string, string> */
    protected function casts(): array
    {
        return [
            'price_in_cents' => 'integer',
            'status' => ProductStatus::class,
        ];
    }

    public function identifier(): int
    {
        $value = $this->getAttribute('id');
        if (!is_int($value)) {
            throw new \LogicException('Persisted products must have an integer identifier.');
        }
        return $value;
    }

    public function name(): string
    {
        $value = $this->getAttribute('name');
        if (!is_string($value)) {
            throw new \LogicException('Persisted products must have a string name.');
        }
        return $value;
    }

    public function priceInCents(): int
    {
        $value = $this->getAttribute('price_in_cents');
        if (!is_int($value)) {
            throw new \LogicException('Persisted products must have an integer price.');
        }
        return $value;
    }

    public function status(): ProductStatus
    {
        $value = $this->getAttribute('status');
        if (!$value instanceof ProductStatus) {
            throw new \LogicException('Persisted products must have a valid status.');
        }
        return $value;
    }
}
