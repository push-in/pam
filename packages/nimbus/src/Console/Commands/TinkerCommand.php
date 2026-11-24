<?php

declare(strict_types=1);

namespace Nimbus\Console\Commands;

use Nimbus\Console\Command;
use Nimbus\Console\Tinker;

/** @internal Built-in command of bin/nimbus; `bin/nimbus tinker` runs PsySH directly. */
final class TinkerCommand extends Command
{
    protected $signature = 'tinker {--execute= : Code to run without the interactive shell}';

    protected $description = 'Opens a PsySH shell with the application ($app, $container, $arcano, $orm).';

    public function handle(): int
    {
        $code = $this->option('execute');
        $shell = Tinker::shell($this->app(), !is_string($code));
        if (!is_string($code)) {
            return $shell->run();
        }
        $result = $shell->execute($code, true);
        if ($result !== null) {
            $this->line(is_scalar($result) ? var_export($result, true) : get_debug_type($result));
        }
        return self::SUCCESS;
    }
}
