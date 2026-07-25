<?php

declare(strict_types=1);

namespace Pam\Laravel\Enums;

enum MigrationFindingStatus: int
{
    case Action = 1;
    case Review = 2;

    public function label(): string
    {
        return strtolower($this->name);
    }
}
