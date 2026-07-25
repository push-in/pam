<?php

declare(strict_types=1);

namespace Pam\Laravel\Enums;

enum CompatibilityStatus: int
{
    case Certified = 1;
    case Provisional = 2;
    case Incompatible = 3;

    public function label(): string
    {
        return strtolower($this->name);
    }
}
