<?php

declare(strict_types=1);

namespace Pam\Octane\Commands\Octane;

use Laravel\Octane\Commands\StatusCommand as OctaneStatusCommand;
use Pam\Octane\Commands\Concerns\ResolvesPamServer;
use Pam\Octane\Server\ServerProcessInspector;

/** `octane:status`, plus `--server=pam`. */
class StatusCommand extends OctaneStatusCommand
{
    use ResolvesPamServer;

    /** @return int */
    public function handle()
    {
        if (!$this->servesPam()) {
            return parent::handle();
        }

        $isRunning = app(ServerProcessInspector::class)->serverIsRunning();

        $isRunning
            ? $this->components->info('Octane server is running.')
            : $this->components->info('Octane server is not running.');

        return $isRunning ? 0 : 1;
    }
}
