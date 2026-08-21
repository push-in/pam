<?php

declare(strict_types=1);

namespace App\Domain\Products;

enum ProductStatus: int
{
    case Active = 1;
    case Archived = 2;
}
