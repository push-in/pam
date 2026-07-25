<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;
use Pam\Laravel\Enums\StackPreset;

final class InstallCommand extends Command
{
    protected $signature = 'pam:install {--preset=api : api, livewire, inertia or realtime} {--force}';
    protected $description = 'Publish PAM Laravel configuration and production manifests';

    public function handle(): int
    {
        $parameters = ['--provider' => 'Pam\\Laravel\\PamServiceProvider'];
        if ($this->option('force')) {
            $parameters['--force'] = true;
        }
        $this->call('vendor:publish', $parameters + ['--tag' => 'pam-config']);
        $this->call('vendor:publish', $parameters + ['--tag' => 'pam-operations']);
        $presetOption = $this->option('preset');
        $preset = StackPreset::fromName(is_string($presetOption) ? $presetOption : '');
        if ($preset === null) {
            $this->error('Preset must be api, livewire, inertia or realtime.');
            return self::INVALID;
        }
        $directory = base_path('.pam');
        if (!is_dir($directory) && !mkdir($directory, 0750, true) && !is_dir($directory)) {
            $this->error('Unable to create .pam.');
            return self::FAILURE;
        }
        file_put_contents($directory.'/laravel.json', json_encode([
            'schema' => 1,
            'preset' => $preset->value,
        ], JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR).PHP_EOL, LOCK_EX);
        $this->info('PAM Laravel is installed. Run `pam artisan pam:check-production`.');

        return self::SUCCESS;
    }
}
