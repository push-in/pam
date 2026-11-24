<?php

declare(strict_types=1);

namespace Nimbus\Console\Commands;

use Nimbus\Console\Command;
use Nimbus\Database\ArcanoDatabaseProvider;
use Nimbus\Database\ArcanoSchemaCompiler;

/** @internal Built-in command of bin/nimbus. */
final class SchemaCacheCommand extends Command
{
    protected $signature = 'schema:cache';

    protected $description = 'Rebuilds the Arcano schema cache that production reads without scanning the entities.';

    public function handle(): int
    {
        [$compiler, $databases] = SchemaCompilerLookup::find($this->app());
        $compiler->clear();
        try {
            $schema = $compiler->schema($databases->manager());
        } finally {
            $databases->releaseCurrent();
            $databases->disconnectIdle();
        }
        $file = $compiler->cacheFile();
        $this->info(sprintf('Schema cached: %d roles%s.', count($schema), $file === null ? '' : " in {$file}"));
        return self::SUCCESS;
    }
}
