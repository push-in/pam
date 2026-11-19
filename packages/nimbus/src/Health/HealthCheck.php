<?php

declare(strict_types=1);

namespace Nimbus\Health;

interface HealthCheck
{
    public function check(): HealthResult;
}

