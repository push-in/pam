<?php

declare(strict_types=1);

namespace Pam\Api\Tenancy;

interface Tenant
{
    public function identifier(): string;
}

