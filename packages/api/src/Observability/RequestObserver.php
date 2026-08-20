<?php

declare(strict_types=1);

namespace Pam\Api\Observability;

interface RequestObserver
{
    public function record(RequestObservation $observation): void;
}

