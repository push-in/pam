<?php

declare(strict_types=1);

namespace Nimbus\Observability;

interface RequestObserver
{
    public function record(RequestObservation $observation): void;
}

