<?php

declare(strict_types=1);

namespace Pam\Octane\Commands\Concerns;

/**
 * Laravel Octane matches `--server` (or `octane.server`) against the servers
 * it ships; the commands of this package answer for "pam" and hand every
 * other server back to Octane unchanged.
 */
trait ResolvesPamServer
{
    protected function servesPam(): bool
    {
        // A signal may arrive before the command's input has been bound.
        $bound = (new \ReflectionProperty(\Illuminate\Console\Command::class, 'input'))->isInitialized($this);
        $server = ($bound ? $this->option('server') : null) ?: config('octane.server');

        return $server === 'pam';
    }
}
