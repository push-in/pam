<?php

declare(strict_types=1);

namespace Pam\Octane\Commands\Octane;

use Laravel\Octane\Commands\ReloadCommand as OctaneReloadCommand;
use Pam\Octane\Commands\Concerns\ResolvesPamServer;
use Pam\Octane\Server\ServerProcessInspector;

/** `octane:reload`, plus `--server=pam`: a zero-downtime worker reload. */
class ReloadCommand extends OctaneReloadCommand
{
    use ResolvesPamServer;

    /** @return int */
    public function handle()
    {
        if (!$this->servesPam()) {
            return parent::handle();
        }

        $inspector = app(ServerProcessInspector::class);

        if (!$inspector->serverIsRunning()) {
            $this->components->error('Octane server is not running.');

            return 1;
        }

        $this->components->info('Reloading workers...');

        $inspector->reloadServer();

        return 0;
    }
}
