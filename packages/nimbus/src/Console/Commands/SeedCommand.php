<?php

declare(strict_types=1);

namespace Nimbus\Console\Commands;

use Nimbus\Console\Command;
use Nimbus\Database\Seeder;
use Nimbus\Database\SeederRunner;

/** @internal Built-in command of bin/nimbus. */
final class SeedCommand extends Command
{
    protected $signature = 'db:seed {--class=Database\Seeders\DatabaseSeeder : The root seeder} {--force}';

    protected $description = 'Runs a database seeder and the seeders it calls.';

    public function handle(): int
    {
        $class = $this->option('class');
        if (!is_string($class) || !class_exists($class) || !is_subclass_of($class, Seeder::class)) {
            $this->error('Seeder ' . (is_string($class) ? $class : '') . ' does not exist or does not extend Nimbus\Database\Seeder.');
            return self::FAILURE;
        }
        foreach ((new SeederRunner($this->app()))->run($class) as $seeder) {
            $this->line("Seeded: {$seeder}");
        }
        return self::SUCCESS;
    }
}
