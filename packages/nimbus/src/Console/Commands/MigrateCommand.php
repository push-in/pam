<?php

declare(strict_types=1);

namespace Nimbus\Console\Commands;

use Nimbus\Console\Command;

/** @internal Built-in command of bin/nimbus. */
final class MigrateCommand extends Command
{
    use UsesMigrations;

    protected $signature = 'migrate {--force : Accepted for Laravel scripts; Nimbus never prompts}';

    protected $description = 'Runs the pending Arcano migrations.';

    public function handle(): int
    {
        $done = $this->migrations()->migrate();
        $this->info($done === [] ? 'Database is already up to date.' : 'Migrated: ' . implode(', ', $done));
        return self::SUCCESS;
    }
}
