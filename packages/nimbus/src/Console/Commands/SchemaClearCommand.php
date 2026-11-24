<?php

declare(strict_types=1);

namespace Nimbus\Console\Commands;

use Nimbus\Console\Command;

/** @internal Built-in command of bin/nimbus. */
final class SchemaClearCommand extends Command
{
    protected $signature = 'schema:clear';

    protected $description = 'Deletes the Arcano schema cache.';

    public function handle(): int
    {
        [$compiler] = SchemaCompilerLookup::find($this->app());
        $compiler->clear();
        $this->info('Schema cache cleared.');
        return self::SUCCESS;
    }
}
