<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;

final class CapacityCommand extends Command
{
    protected $signature = 'pam:capacity {--memory-mb=512} {--worker-mb=96} {--reserve-percent=20}';
    protected $description = 'Estimate a conservative PAM worker count from a memory budget';

    public function handle(): int
    {
        $memory = max(1, (int) $this->option('memory-mb'));
        $worker = max(1, (int) $this->option('worker-mb'));
        $reserve = min(90, max(0, (int) $this->option('reserve-percent')));
        $usable = (int) floor($memory * (100 - $reserve) / 100);
        $workers = max(1, intdiv($usable, $worker));
        $this->table(['Memory', 'Reserve', 'Per worker', 'Recommended workers'], [[
            "{$memory} MiB", "{$reserve}%", "{$worker} MiB", $workers,
        ]]);
        $this->warn('Validate this estimate with production-like load and `pam benchmark`.');

        return self::SUCCESS;
    }
}
