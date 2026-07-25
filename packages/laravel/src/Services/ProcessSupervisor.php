<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use InvalidArgumentException;
use RuntimeException;

final readonly class ProcessSupervisor
{
    /** @return array<string, array{command: list<string>, instances: int}> */
    public function manifest(): array
    {
        $path = (string) config('pam.supervisor.manifest');
        $decoded = json_decode((string) @file_get_contents($path), true, flags: JSON_THROW_ON_ERROR);
        if (!is_array($decoded) || !isset($decoded['processes']) || !is_array($decoded['processes'])) {
            throw new RuntimeException("Invalid PAM process manifest: {$path}");
        }

        $processes = [];
        foreach ($decoded['processes'] as $name => $definition) {
            if (!preg_match('/^[a-z][a-z0-9_-]*$/', (string) $name)) {
                throw new InvalidArgumentException("Invalid process name: {$name}");
            }
            if (!is_array($definition)) {
                throw new InvalidArgumentException("Process {$name} must be an object.");
            }
            $command = $definition['command'] ?? null;
            if (!is_array($command) || $command === [] || array_filter($command, 'is_string') !== $command) {
                throw new InvalidArgumentException("Process {$name} must define a non-empty command array.");
            }
            $processes[(string) $name] = [
                'command' => array_values($command),
                'instances' => $this->desiredInstances((string) $name, max(1, (int) ($definition['instances'] ?? 1))),
            ];
        }

        return $processes;
    }

    /** @return list<array{name: string, instance: int, pid: int, running: bool}> */
    public function status(): array
    {
        $rows = [];
        foreach ($this->manifest() as $name => $definition) {
            for ($instance = 1; $instance <= $definition['instances']; ++$instance) {
                $pid = $this->readPid($name, $instance);
                $rows[] = compact('name', 'instance', 'pid') + ['running' => $this->running($pid)];
            }
        }

        return $rows;
    }

    public function start(?string $only = null): void
    {
        foreach ($this->manifest() as $name => $definition) {
            if ($only !== null && $name !== $only) {
                continue;
            }
            for ($instance = 1; $instance <= $definition['instances']; ++$instance) {
                if ($this->running($this->readPid($name, $instance))) {
                    continue;
                }
                $this->spawn($name, $instance, $definition['command']);
            }
        }
    }

    public function stop(?string $only = null): void
    {
        foreach ($this->status() as $process) {
            if (($only === null || $process['name'] === $only) && $process['running']) {
                posix_kill($process['pid'], SIGTERM);
            }
        }
    }

    public function restart(?string $only = null): void
    {
        $this->stop($only);
        usleep(200_000);
        $this->start($only);
    }

    public function scale(string $name, int $instances): void
    {
        $manifest = $this->manifest();
        if (!isset($manifest[$name]) || $instances < 1 || $instances > 128) {
            throw new InvalidArgumentException('Scale requires a known process and 1..128 instances.');
        }
        $this->stop($name);
        $this->ensureDirectory((string) config('pam.supervisor.state_path'));
        $overrides = $this->scaleOverrides();
        $overrides[$name] = $instances;
        file_put_contents($this->scalePath(), json_encode($overrides, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR), LOCK_EX);
        usleep(200_000);
        $this->start($name);
    }

    public function logPath(string $name, int $instance = 1): string
    {
        if (!isset($this->manifest()[$name])) {
            throw new InvalidArgumentException("Unknown process: {$name}");
        }

        return rtrim((string) config('pam.supervisor.log_path'), '/')."/{$name}-{$instance}.log";
    }

    /** @param list<string> $command */
    private function spawn(string $name, int $instance, array $command): void
    {
        $state = (string) config('pam.supervisor.state_path');
        $logs = (string) config('pam.supervisor.log_path');
        $this->ensureDirectory($state);
        $this->ensureDirectory($logs);
        $executable = implode(' ', array_map('escapeshellarg', $command));
        $log = escapeshellarg("{$logs}/{$name}-{$instance}.log");
        $shell = "{$executable} >> {$log} 2>&1 & echo $!";
        exec($shell, $output, $code);
        $pid = (int) ($output[0] ?? 0);
        if ($code !== 0 || $pid < 1) {
            throw new RuntimeException("Unable to start {$name}:{$instance}.");
        }
        file_put_contents($this->pidPath($name, $instance), (string) $pid, LOCK_EX);
    }

    private function readPid(string $name, int $instance): int
    {
        return (int) @file_get_contents($this->pidPath($name, $instance));
    }

    private function running(int $pid): bool
    {
        return $pid > 0 && function_exists('posix_kill') && posix_kill($pid, 0);
    }

    private function pidPath(string $name, int $instance): string
    {
        return rtrim((string) config('pam.supervisor.state_path'), '/')."/{$name}-{$instance}.pid";
    }

    private function desiredInstances(string $name, int $fallback): int
    {
        return max(1, min(128, (int) ($this->scaleOverrides()[$name] ?? $fallback)));
    }

    /** @return array<string, int> */
    private function scaleOverrides(): array
    {
        $decoded = json_decode((string) @file_get_contents($this->scalePath()), true);

        return is_array($decoded) ? $decoded : [];
    }

    private function scalePath(): string
    {
        return rtrim((string) config('pam.supervisor.state_path'), '/').'/scale.json';
    }

    private function ensureDirectory(string $path): void
    {
        if (!is_dir($path) && !mkdir($path, 0750, true) && !is_dir($path)) {
            throw new RuntimeException("Unable to create {$path}.");
        }
    }
}
