<?php

declare(strict_types=1);

namespace Pam\Octane\Commands\Octane;

use Laravel\Octane\Commands\InstallCommand as OctaneInstallCommand;
use Pam\Octane\Commands\Concerns\InstallsPamDependencies;
use Throwable;

use function Laravel\Prompts\select;

/**
 * `octane:install`, plus `--server=pam` (and "pam" in the server prompt):
 * finds the PAM binary or offers PAM's official install script.
 */
class InstallCommand extends OctaneInstallCommand
{
    use InstallsPamDependencies;

    /** @return int */
    public function handle()
    {
        $server = $this->option('server') ?: select(
            label: 'Which application server you would like to use?',
            options: ['frankenphp', 'roadrunner', 'swoole', 'pam'],
            default: 'frankenphp'
        );

        if ($server !== 'pam') {
            $this->input->setOption('server', $server);

            return parent::handle();
        }

        if (!$this->installPamServer()) {
            return 1;
        }

        $this->updateEnvironmentFile('pam');

        $this->callSilent('vendor:publish', [
            '--tag' => 'octane-config',
            '--force' => $this->option('force'),
        ]);

        $this->components->info('Octane installed successfully.');
        $this->newLine();

        return 0;
    }

    /** Install the PAM server. */
    public function installPamServer(): bool
    {
        try {
            $this->ensurePamBinaryMeetsRequirements($this->ensurePamBinaryIsInstalled());
        } catch (Throwable $e) {
            $this->components->error($e->getMessage());

            return false;
        }

        return true;
    }
}
