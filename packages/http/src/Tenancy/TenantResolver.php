<?php

declare(strict_types=1);

namespace Pam\Api\Tenancy;

use Pam\Http\Request;

interface TenantResolver
{
    public function resolve(Request $request): ?Tenant;
}

