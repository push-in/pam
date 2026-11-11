<?php

declare(strict_types=1);

namespace Pam\Octane\Server;

use Laravel\Octane\Contracts\ServerProcessInspector as ServerProcessInspectorContract;
use Laravel\Octane\PosixExtension;

/**
 * Reload and stop go to the `pam start` supervisor: SIGHUP replaces every
 * worker make-before-break (zero-downtime), SIGTERM drains and stops.
 */
class ServerProcessInspector implements ServerProcessInspectorContract
{
    public function __construct(
        protected ServerStateFile $serverStateFile,
        protected PosixExtension $posix,
    ) {
    }

    public function serverIsRunning(): bool
    {
        $pid = $this->serverStateFile->read()['masterProcessId'];

        return $pid !== null && (bool) $this->posix->kill($pid, 0);
    }

    public function reloadServer(): void
    {
        $pid = $this->serverStateFile->read()['masterProcessId'];

        if ($pid !== null) {
            $this->posix->kill($pid, \defined('SIGHUP') ? SIGHUP : 1);
        }
    }

    public function stopServer(): bool
    {
        $pid = $this->serverStateFile->read()['masterProcessId'];

        return $pid !== null && (bool) $this->posix->kill($pid, \defined('SIGTERM') ? SIGTERM : 15);
    }
}
