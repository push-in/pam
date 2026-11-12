<?php

declare(strict_types=1);

namespace App\Models;

use App\Domain\Products\ProductStatus;
use Arcano\Annotated\Annotation\Column;
use Arcano\Annotated\Annotation\Entity;

#[Entity(role: 'product', table: 'products')]
class Product
{
    #[Column(type: 'primary')]
    public ?int $id = null;

    #[Column(type: 'datetime', name: 'created_at')]
    public \DateTimeImmutable $createdAt;

    #[Column(type: 'datetime', name: 'updated_at')]
    public \DateTimeImmutable $updatedAt;

    public function __construct(
        #[Column(type: 'string')]
        public string $name,
        #[Column(type: 'integer', name: 'price_in_cents')]
        public int $priceInCents,
        #[Column(type: 'tinyInteger', default: 1, typecast: ProductStatus::class)]
        public ProductStatus $status = ProductStatus::Active,
    ) {
        $this->createdAt = $this->updatedAt = new \DateTimeImmutable();
    }

    public function identifier(): int
    {
        return $this->id ?? throw new \LogicException('Persisted products must have an integer identifier.');
    }
}
