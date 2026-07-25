<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;
use Pam\Laravel\Services\NightwatchIntegration;
use Pam\Laravel\Support\ConfigValue;
use Throwable;

final class NightwatchCommand extends Command
{
    protected $signature = 'pam:nightwatch {--install-process} {--json}';
    protected $description = 'Validate and configure Laravel Nightwatch for persistent PAM workers';

    public function handle(NightwatchIntegration $integration): int
    {
        if ($this->option('install-process')) {
            try {
                $this->installProcess();
            } catch (Throwable $exception) {
                $this->error($exception->getMessage());
                return self::FAILURE;
            }
        }
        $report = $integration->inspect();
        if ($this->option('json')) {
            $this->line((string) json_encode($report, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
        } else {
            $this->table(['Check', 'Result'], [
                ['Package installed', $report['installed'] ? 'yes' : 'no'],
                ['Agent command', $report['command'] ? 'yes' : 'no'],
                ['Token configured', $report['configured'] ? 'yes' : 'no'],
                ['Managed process', $report['process'] ? 'yes' : 'no'],
            ]);
            foreach ($report['recommendations'] as $recommendation) {
                $this->warn($recommendation);
            }
        }

        return $report['installed'] && $report['command'] && $report['configured'] && $report['process']
            ? self::SUCCESS
            : self::FAILURE;
    }

    private function installProcess(): void
    {
        $path = ConfigValue::string('pam.supervisor.manifest');
        $manifest = json_decode((string) @file_get_contents($path), true, flags: JSON_THROW_ON_ERROR);
        if (!is_array($manifest) || !is_array($manifest['processes'] ?? null)) {
            throw new \RuntimeException("Invalid PAM process manifest: {$path}");
        }
        $manifest['processes']['nightwatch'] = [
            'command' => ['pam', 'artisan', 'nightwatch:agent'],
            'instances' => 1,
        ];
        $written = file_put_contents(
            $path,
            json_encode($manifest, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES).PHP_EOL,
            LOCK_EX,
        );
        if ($written === false) {
            throw new \RuntimeException("Unable to write the PAM process manifest: {$path}");
        }
        $this->info('Nightwatch agent added to the PAM process manifest.');
    }
}
