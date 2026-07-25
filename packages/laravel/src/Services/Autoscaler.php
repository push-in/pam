<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use Pam\Laravel\Support\ConfigValue;

final readonly class Autoscaler
{
    public function __construct(private ProcessSupervisor $supervisor)
    {
    }

    public function reconcile(string $process, float $cpuPercent, float $p95Milliseconds): int
    {
        $manifest = $this->supervisor->manifest();
        $current = $manifest[$process]['instances'] ?? 0;
        if ($current < 1) {
            throw new \InvalidArgumentException("Unknown process: {$process}");
        }
        $minimum = max(1, ConfigValue::int('pam.autoscaling.min_instances', 1));
        $maximum = max($minimum, min(128, ConfigValue::int('pam.autoscaling.max_instances', 16)));
        $targetCpu = max(1.0, min(99.0, ConfigValue::float('pam.autoscaling.target_cpu_percent', 65)));
        $targetP95 = max(1.0, ConfigValue::float('pam.autoscaling.target_p95_ms', 250));
        $scaleDownCpu = max(1.0, $targetCpu * 0.5);
        $scaleDownP95 = max(1.0, $targetP95 * 0.5);
        $desired = $current;
        if (($cpuPercent > $targetCpu || $p95Milliseconds > $targetP95) && $current < $maximum) {
            $desired = min($maximum, $current + max(1, (int) ceil($current * 0.25)));
        } elseif ($cpuPercent < $scaleDownCpu && $p95Milliseconds < $scaleDownP95 && $current > $minimum) {
            $desired = max($minimum, $current - 1);
        }
        if ($desired !== $current) {
            $cooldown = max(0, ConfigValue::int('pam.autoscaling.cooldown_seconds', 60));
            if (time() - $this->lastScaledAt($process) < $cooldown) {
                return $current;
            }
            $this->supervisor->scale($process, $desired);
            $this->recordScale($process);
        }

        return $desired;
    }

    private function lastScaledAt(string $process): int
    {
        $decoded = json_decode((string) @file_get_contents($this->statePath()), true);

        return is_array($decoded) && is_int($decoded[$process] ?? null) ? $decoded[$process] : 0;
    }

    private function recordScale(string $process): void
    {
        $path = $this->statePath();
        $directory = dirname($path);
        if (!is_dir($directory) && !mkdir($directory, 0750, true) && !is_dir($directory)) {
            throw new \RuntimeException("Unable to create {$directory}.");
        }
        $decoded = json_decode((string) @file_get_contents($path), true);
        $state = is_array($decoded) ? $decoded : [];
        $state[$process] = time();
        if (file_put_contents($path, json_encode($state, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR), LOCK_EX) === false) {
            throw new \RuntimeException('Unable to persist autoscaling state.');
        }
    }

    private function statePath(): string
    {
        return rtrim(ConfigValue::string('pam.supervisor.state_path'), '/').'/autoscale.json';
    }
}
