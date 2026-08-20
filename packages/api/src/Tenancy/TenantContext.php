<?php

declare(strict_types=1);

namespace Pam\Api\Tenancy;

final readonly class TenantContext
{
    public function __construct(public Tenant $tenant)
    {
    }
}

