<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;
use Pam\Laravel\Services\ObservabilityRegistry;

final class LeaksCommand extends Command
{
    protected $signature = 'pam:leaks {--json}';
    protected $description = 'Inspect persistent-worker state violations and retained memory';

    public function handle(ObservabilityRegistry $registry): int
    {
        $snapshot = $registry->snapshot();
        if ($this->option('json')) {
            $this->line((string) json_encode($snapshot, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
        } else {
            $this->components->twoColumnDetail('Current memory', $this->formatBytes($registry->memoryBytes()));
            $this->components->twoColumnDetail('Peak memory', $this->formatBytes($registry->peakMemoryBytes()));
            $this->components->twoColumnDetail('State violations', (string) $registry->stateViolationCount());
            $this->components->twoColumnDetail('Slow queries retained', (string) $registry->slowQueryCount());
        }

        return $registry->hasStateViolations() ? self::FAILURE : self::SUCCESS;
    }

    private function formatBytes(int $bytes): string
    {
        return number_format($bytes / 1_048_576, 2).' MiB';
    }
}
