<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;
use Pam\Laravel\Services\ProcessSupervisor;

final class SupervisorCommand extends Command
{
    protected $signature = 'pam:process {action : up, status, restart, stop, scale or logs} {name?} {value?}';
    protected $description = 'Manage the PAM application process manifest';

    public function handle(ProcessSupervisor $supervisor): int
    {
        $actionArgument = $this->argument('action');
        $action = is_string($actionArgument) ? $actionArgument : '';
        $name = $this->argument('name');
        if (!in_array($action, ['up', 'status', 'restart', 'stop', 'scale', 'logs'], true)) {
            $this->error('Action must be up, status, restart, stop, scale or logs.');
            return self::INVALID;
        }
        if ($action === 'scale') {
            $value = $this->argument('value');
            $supervisor->scale(is_string($name) ? $name : '', is_numeric($value) ? (int) $value : 0);
        } elseif ($action === 'logs') {
            $value = $this->argument('value');
            $path = $supervisor->logPath(is_string($name) ? $name : '', max(1, is_numeric($value) ? (int) $value : 1));
            $this->line((string) @file_get_contents($path));
            return self::SUCCESS;
        } elseif ($action === 'up') {
            $supervisor->start(is_string($name) ? $name : null);
        } elseif ($action === 'restart') {
            $supervisor->restart(is_string($name) ? $name : null);
        } elseif ($action === 'stop') {
            $supervisor->stop(is_string($name) ? $name : null);
        }
        $this->table(['Process', 'Instance', 'PID', 'State'], array_map(
            fn (array $row) => [$row['name'], $row['instance'], $row['pid'], $row['running'] ? 'running' : 'stopped'],
            $supervisor->status(),
        ));

        return self::SUCCESS;
    }
}
