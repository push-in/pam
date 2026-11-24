<?php

declare(strict_types=1);

namespace Nimbus\Console\Commands;

use Nimbus\Console\Command;

/** @internal Built-in command of bin/nimbus. */
final class MigrateStatusCommand extends Command
{
    use UsesMigrations;

    protected $signature = 'migrate:status';

    protected $description = 'Lists the Arcano migrations and when they ran.';

    public function handle(): int
    {
        $rows = [];
        foreach ($this->migrations()->status() as $migration) {
            $rows[] = [$migration['name'], $migration['executed_at']?->format('Y-m-d H:i:s') ?? 'Pending'];
        }
        if ($rows === []) {
            $this->info('No migrations found.');
            return self::SUCCESS;
        }
        $this->table(['Migration', 'Ran at'], $rows);
        return self::SUCCESS;
    }
}
