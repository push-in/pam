<?php

declare(strict_types=1);

namespace Pam\Laravel\Contracts;

use Pam\Laravel\ValueObjects\TelemetrySpan;

interface TelemetryExporter
{
    /** @param list<TelemetrySpan> $spans */
    public function export(array $spans): void;
}
