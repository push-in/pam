<?php

declare(strict_types=1);

namespace Pam\Api\Auth;

interface Principal
{
    public function identifier(): string;

    public function can(string $ability): bool;
}

