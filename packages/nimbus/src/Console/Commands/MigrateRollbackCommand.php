<?php

declare(strict_types=1);

namespace Nimbus\Console\Commands;

use Nimbus\Console\Command;

/** @internal Built-in command of bin/nimbus. */
final class MigrateRollbackCommand extends Command
{
    use UsesMigrations;

    protected $signature = 'migrate:rollback {--step=1 : Migrations to roll back} {--force}';

    protected $description = 'Rolls back the last Arcano migrations.';

    public function handle(): int
    {
        $step = $this->option('step');
        $done = $this->migrations()->rollback(max(1, is_numeric($step) ? (int) $step : 1));
        $this->info($done === [] ? 'Nothing to roll back.' : 'Rolled back: ' . implode(', ', $done));
        return self::SUCCESS;
    }
}
