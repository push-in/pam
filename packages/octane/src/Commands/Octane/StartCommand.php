<?php

declare(strict_types=1);

namespace Pam\Octane\Commands\Octane;

use Laravel\Octane\Commands\StartCommand as OctaneStartCommand;
use Pam\Octane\Commands\Concerns\ResolvesPamServer;

/** `octane:start`, plus `--server=pam` (or OCTANE_SERVER=pam). */
class StartCommand extends OctaneStartCommand
{
    use ResolvesPamServer;

    /** @return int */
    public function handle()
    {
        if (!$this->servesPam()) {
            return parent::handle();
        }

        return $this->call('octane:pam', [
            '--host' => $this->getHost(),
            '--port' => $this->getPort(),
            '--workers' => $this->option('workers') ?: config('octane.workers', 'auto'),
            '--task-workers' => $this->option('task-workers') ?: config('octane.task_workers', 0),
            '--max-requests' => $this->option('max-requests') ?: config('octane.max_requests', 500),
            '--watch' => $this->option('watch'),
            '--poll' => $this->option('poll'),
        ]);
    }
}
