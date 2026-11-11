<?php

declare(strict_types=1);

namespace Pam\Octane\Commands\Octane;

use Laravel\Octane\Commands\StopCommand as OctaneStopCommand;
use Pam\Octane\Commands\Concerns\ResolvesPamServer;
use Pam\Octane\Server\ServerProcessInspector;
use Pam\Octane\Server\ServerStateFile;

/** `octane:stop`, plus `--server=pam`. */
class StopCommand extends OctaneStopCommand
{
    use ResolvesPamServer;

    /** @return int */
    public function handle()
    {
        if (!$this->servesPam()) {
            return parent::handle();
        }

        $inspector = app(ServerProcessInspector::class);
        $stateFile = app(ServerStateFile::class);

        if (!$inspector->serverIsRunning()) {
            $stateFile->delete();

            $this->components->error('PAM server is not running.');

            return 1;
        }

        $this->components->info('Stopping server...');

        $inspector->stopServer();

        $stateFile->delete();

        return 0;
    }
}
