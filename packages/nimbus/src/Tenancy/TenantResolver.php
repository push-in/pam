<?php

declare(strict_types=1);

namespace Nimbus\Tenancy;

use Pam\Http\Request;

interface TenantResolver
{
    public function resolve(Request $request): ?Tenant;
}

