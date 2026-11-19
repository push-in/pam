<?php

declare(strict_types=1);

namespace Nimbus\Auth;

interface Principal
{
    public function identifier(): string;

    public function can(string $ability): bool;
}

