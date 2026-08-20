<?php

declare(strict_types=1);

namespace Pam\Api\Health;

interface HealthCheck
{
    public function check(): HealthResult;
}

