<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;
use Pam\Laravel\Services\HealthReporter;

final class HealthCommand extends Command
{
    protected $signature = 'pam:health';
    protected $description = 'Print PAM Laravel health and runtime telemetry';

    public function handle(HealthReporter $reporter): int
    {
        $report = $reporter->report();
        $this->line((string) json_encode($report, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));

        return $report['ok'] ? self::SUCCESS : self::FAILURE;
    }
}
