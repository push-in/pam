<?php

declare(strict_types=1);

namespace Nimbus\Tenancy;

interface Tenant
{
    public function identifier(): string;
}

