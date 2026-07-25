<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;
use Pam\Laravel\Services\Autoscaler;
use Pam\Laravel\Services\AutoscaleMetricsClient;
use Pam\Laravel\Support\ConfigValue;
use Throwable;

final class AutoscaleCommand extends Command
{
    protected $signature = 'pam:autoscale
        {process=queue}
        {--cpu=0 : Current average CPU percentage}
        {--p95=0 : Current p95 latency in milliseconds}
        {--metrics-url= : JSON endpoint with cpuPercent and p95Milliseconds}
        {--watch : Reconcile continuously}
        {--interval=15 : Watch interval in seconds}';

    protected $description = 'Reconcile local PAM worker capacity against CPU and p95 targets';

    public function handle(Autoscaler $autoscaler, AutoscaleMetricsClient $metrics): int
    {
        $process = $this->argument('process');
        if (!is_string($process) || !preg_match('/^[a-z][a-z0-9_-]*$/', $process)) {
            $this->error('Process name is invalid.');
            return self::INVALID;
        }
        $interval = max(5, min(300, (int) $this->option('interval')));
        $urlOption = $this->option('metrics-url');
        $metricsUrl = is_string($urlOption) && $urlOption !== ''
            ? $urlOption
            : ConfigValue::string('pam.autoscaling.metrics_url');
        if ($this->option('watch') && $metricsUrl === '') {
            $this->error('--watch requires --metrics-url or PAM_AUTOSCALE_METRICS_URL.');
            return self::INVALID;
        }
        do {
            try {
                $sample = $metricsUrl !== ''
                    ? $metrics->read($metricsUrl)
                    : ['cpu' => (float) $this->option('cpu'), 'p95' => (float) $this->option('p95')];
                $desired = $autoscaler->reconcile($process, $sample['cpu'], $sample['p95']);
            } catch (Throwable $exception) {
                $this->error($exception->getMessage());
                return self::FAILURE;
            }
            $this->info("{$process} desired instances: {$desired}");
            if (!$this->option('watch')) {
                break;
            }
            sleep($interval);
        } while (true);

        return self::SUCCESS;
    }
}
