<?php

declare(strict_types=1);

namespace Pam\Laravel\Services;

use Composer\InstalledVersions;
use Pam\Laravel\Support\ConfigValue;
use Symfony\Component\Console\Application;

final readonly class NightwatchIntegration
{
    /** @return array{installed: bool, command: bool, configured: bool, process: bool, recommendations: list<string>} */
    public function inspect(): array
    {
        $installed = class_exists('Laravel\\Nightwatch\\NightwatchServiceProvider')
            || class_exists('Laravel\\Nightwatch\\Nightwatch')
            || (class_exists(InstalledVersions::class) && InstalledVersions::isInstalled('laravel/nightwatch'));
        $command = $this->artisanCommandExists('nightwatch:agent');
        $configured = ConfigValue::string('pam.nightwatch.token') !== '';
        $process = false;
        try {
            $manifest = app(ProcessSupervisor::class)->manifest();
            $process = isset($manifest['nightwatch']);
        } catch (\Throwable) {
            // An unpublished or intentionally absent process manifest is reported below.
        }
        $recommendations = [];
        if (!$installed) {
            $recommendations[] = 'Install laravel/nightwatch using a version compatible with your Laravel release.';
        }
        if ($installed && !$configured) {
            $recommendations[] = 'Set NIGHTWATCH_TOKEN in the deployment secret store.';
        }
        if ($installed && !$process) {
            $recommendations[] = 'Add one managed `nightwatch:agent` process per application.';
        }

        return compact('installed', 'command', 'configured', 'process', 'recommendations');
    }

    private function artisanCommandExists(string $name): bool
    {
        try {
            $artisan = app('artisan');

            return $artisan instanceof Application && $artisan->has($name);
        } catch (\Throwable) {
            return false;
        }
    }
}
