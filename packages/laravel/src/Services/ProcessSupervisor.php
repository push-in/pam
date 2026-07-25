<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use InvalidArgumentException;
use Pam\Laravel\Support\ConfigValue;
use RuntimeException;

final readonly class ProcessSupervisor
{
    /** @return array<string, array{command: list<string>, instances: int}> */
    public function manifest(): array
    {
        $path = ConfigValue::string('pam.supervisor.manifest');
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
            $configuredInstances = $definition['instances'] ?? 1;
            $instances = is_int($configuredInstances)
                ? $configuredInstances
                : (is_numeric($configuredInstances) ? (int) $configuredInstances : 1);
            $processes[(string) $name] = [
                'command' => array_values($command),
                'instances' => $this->desiredInstances((string) $name, max(1, $instances)),
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
                $state = $this->readProcessState($name, $instance);
                $pid = $state['pid'];
                $rows[] = compact('name', 'instance', 'pid') + ['running' => $this->running($state)];
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
                $state = $this->readProcessState($name, $instance);
                if ($this->running($state)) {
                    continue;
                }
                if ($state['pid'] > 0 && $state['startTicks'] === 0 && posix_kill($state['pid'], 0)) {
                    throw new RuntimeException("Legacy PID state for {$name}:{$instance} cannot be verified; inspect the process before removing its PID file.");
                }
                $this->spawn($name, $instance, $definition['command']);
            }
        }
    }

    public function stop(?string $only = null): void
    {
        /** @var list<array{name: string, instance: int, state: array{pid: int, startTicks: int}}> $targets */
        $targets = [];
        foreach ($this->manifest() as $name => $definition) {
            if ($only !== null && $name !== $only) {
                continue;
            }
            for ($instance = 1; $instance <= $definition['instances']; ++$instance) {
                $state = $this->readProcessState($name, $instance);
                if ($this->running($state)) {
                    posix_kill($state['pid'], SIGTERM);
                    $targets[] = compact('name', 'instance', 'state');
                }
            }
        }
        $deadline = microtime(true) + max(1, min(60, ConfigValue::int('pam.supervisor.stop_timeout_seconds', 10)));
        do {
            $running = array_filter($targets, fn (array $target): bool => $this->running($target['state']));
            if ($running === [] || microtime(true) >= $deadline) {
                break;
            }
            usleep(50_000);
        } while (true);
        foreach ($targets as $target) {
            if ($this->running($target['state'])) {
                posix_kill($target['state']['pid'], SIGKILL);
            }
            @unlink($this->pidPath($target['name'], $target['instance']));
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
        $this->ensureDirectory(ConfigValue::string('pam.supervisor.state_path'));
        $overrides = $this->scaleOverrides();
        $overrides[$name] = $instances;
        if (file_put_contents(
            $this->scalePath(),
            json_encode($overrides, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR),
            LOCK_EX,
        ) === false) {
            throw new RuntimeException('Unable to persist process scale overrides.');
        }
        usleep(200_000);
        $this->start($name);
    }

    public function logPath(string $name, int $instance = 1): string
    {
        if (!isset($this->manifest()[$name])) {
            throw new InvalidArgumentException("Unknown process: {$name}");
        }

        return rtrim(ConfigValue::string('pam.supervisor.log_path'), '/')."/{$name}-{$instance}.log";
    }

    /** @param list<string> $command */
    private function spawn(string $name, int $instance, array $command): void
    {
        $state = ConfigValue::string('pam.supervisor.state_path');
        $logs = ConfigValue::string('pam.supervisor.log_path');
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
        $startTicks = $this->processStartTicks($pid);
        if ($startTicks < 1) {
            posix_kill($pid, SIGTERM);
            throw new RuntimeException("Unable to verify {$name}:{$instance} after startup.");
        }
        if (file_put_contents($this->pidPath($name, $instance), json_encode([
            'pid' => $pid,
            'startTicks' => $startTicks,
        ], JSON_THROW_ON_ERROR), LOCK_EX) === false) {
            posix_kill($pid, SIGTERM);
            throw new RuntimeException("Unable to persist process state for {$name}:{$instance}.");
        }
    }

    /** @return array{pid: int, startTicks: int} */
    private function readProcessState(string $name, int $instance): array
    {
        $contents = (string) @file_get_contents($this->pidPath($name, $instance));
        $decoded = json_decode($contents, true);
        if (is_array($decoded) && is_int($decoded['pid'] ?? null) && is_int($decoded['startTicks'] ?? null)) {
            return ['pid' => $decoded['pid'], 'startTicks' => $decoded['startTicks']];
        }

        return ['pid' => is_numeric($contents) ? (int) $contents : 0, 'startTicks' => 0];
    }

    /** @param array{pid: int, startTicks: int} $state */
    private function running(array $state): bool
    {
        return $state['pid'] > 0
            && $state['startTicks'] > 0
            && function_exists('posix_kill')
            && posix_kill($state['pid'], 0)
            && $this->processStartTicks($state['pid']) === $state['startTicks'];
    }

    private function processStartTicks(int $pid): int
    {
        $stat = (string) @file_get_contents("/proc/{$pid}/stat");
        $commandEnd = strrpos($stat, ') ');
        if ($commandEnd === false) {
            return 0;
        }
        $fields = preg_split('/\s+/', substr($stat, $commandEnd + 2));

        return is_array($fields) && isset($fields[19]) && is_numeric($fields[19])
            ? (int) $fields[19]
            : 0;
    }

    private function pidPath(string $name, int $instance): string
    {
        return rtrim(ConfigValue::string('pam.supervisor.state_path'), '/')."/{$name}-{$instance}.pid";
    }

    private function desiredInstances(string $name, int $fallback): int
    {
        return max(1, min(128, (int) ($this->scaleOverrides()[$name] ?? $fallback)));
    }

    /** @return array<string, int> */
    private function scaleOverrides(): array
    {
        $decoded = json_decode((string) @file_get_contents($this->scalePath()), true);
        if (!is_array($decoded)) {
            return [];
        }
        $overrides = [];
        foreach ($decoded as $name => $instances) {
            if (is_string($name) && is_int($instances)) {
                $overrides[$name] = $instances;
            }
        }

        return $overrides;
    }

    private function scalePath(): string
    {
        return rtrim(ConfigValue::string('pam.supervisor.state_path'), '/').'/scale.json';
    }

    private function ensureDirectory(string $path): void
    {
        if (!is_dir($path) && !mkdir($path, 0750, true) && !is_dir($path)) {
            throw new RuntimeException("Unable to create {$path}.");
        }
    }
}
