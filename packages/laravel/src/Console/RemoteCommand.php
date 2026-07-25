<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;
use Pam\Laravel\Enums\RemoteAction;
use Pam\Laravel\Services\RemoteControlClient;
use Throwable;

final class RemoteCommand extends Command
{
    protected $signature = 'pam:remote
        {action : deploy, rollback, status, logs, top, workers, queues, scheduler or scale}
        {target=production}
        {--process=}
        {--instances=}
        {--release=}
        {--lines=200}
        {--json}';

    protected $description = 'Operate PAM Cloud or Laravel Forge deployment targets';

    public function handle(RemoteControlClient $client): int
    {
        $actionName = $this->argument('action');
        $action = RemoteAction::fromName(is_string($actionName) ? $actionName : '');
        $target = $this->argument('target');
        if ($action === null || !is_string($target)) {
            $this->error('Invalid remote action or target.');
            return self::INVALID;
        }
        $parameters = [];
        if ($action === RemoteAction::Scale) {
            $process = $this->option('process');
            $instances = $this->option('instances');
            if (!is_string($process) || !preg_match('/^[a-z][a-z0-9_-]*$/', $process)
                || !is_numeric($instances) || (int) $instances < 1 || (int) $instances > 128) {
                $this->error('Scale requires --process and --instances=1..128.');
                return self::INVALID;
            }
            $parameters = ['process' => $process, 'instances' => (int) $instances];
        }
        if ($action === RemoteAction::Deploy && is_string($this->option('release')) && $this->option('release') !== '') {
            $parameters['release'] = (string) $this->option('release');
        }
        if ($action === RemoteAction::Logs) {
            $parameters['lines'] = max(1, min(10_000, (int) $this->option('lines')));
        }

        try {
            $result = $client->execute($action, $target, $parameters);
        } catch (Throwable $exception) {
            $this->error($exception->getMessage());
            return self::FAILURE;
        }
        $this->line((string) json_encode($result, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));

        return self::SUCCESS;
    }
}
