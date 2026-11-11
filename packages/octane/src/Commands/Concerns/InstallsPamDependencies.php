<?php

declare(strict_types=1);

namespace Pam\Octane\Commands\Concerns;

use RuntimeException;
use Symfony\Component\Process\Process;

use function Laravel\Prompts\confirm;

trait InstallsPamDependencies
{
    use FindsPamBinary;

    /** The minimum PAM version for `octane:start --server=pam`. */
    protected string $requiredPamVersion = '2.2.15';

    /** PAM's official install script, which verifies the release checksum. */
    protected string $pamInstallScriptUrl = 'https://github.com/push-in/pam/releases/latest/download/install.sh';

    protected function ensurePamBinaryIsInstalled(): string
    {
        if (($pamBinary = $this->findPamBinary()) !== null) {
            return $pamBinary;
        }

        if (!$this->pamBinariesAreAvailable()) {
            throw new RuntimeException('PAM binaries are only available for Linux and macOS. On Windows, run Octane in WSL 2 or Docker (Linux), then install PAM there.');
        }

        if (confirm('Unable to locate PAM binary. Should Octane install PAM using its official install script?', true)) {
            $this->installPamBinary();
        }

        if (($pamBinary = $this->findPamBinary()) === null) {
            throw new RuntimeException("Unable to locate PAM binary. Please install PAM using [curl -fsSL {$this->pamInstallScriptUrl} | sh].");
        }

        return $pamBinary;
    }

    protected function pamBinariesAreAvailable(): bool
    {
        return in_array(PHP_OS_FAMILY, ['Linux', 'Darwin'], true);
    }

    protected function installPamBinary(): void
    {
        if (!$this->pamBinariesAreAvailable()) {
            throw new RuntimeException('PAM binaries are only available for Linux and macOS. On Windows, run Octane in WSL 2 or Docker (Linux), then install PAM there.');
        }

        Process::fromShellCommandline(
            'curl --proto "=https" --tlsv1.2 -fsSL "$PAM_INSTALL_SCRIPT" | sh',
            base_path(),
            ['PAM_INSTALL_SCRIPT' => $this->pamInstallScriptUrl],
        )->setTimeout(300)->mustRun(function (string $type, string $buffer): void {
            $this->output->write($buffer);
        });
    }

    protected function ensurePamBinaryMeetsRequirements(string $pamBinary): void
    {
        $process = new Process([$pamBinary, '--version'], base_path());
        $process->run();

        if (preg_match('/(\d+\.\d+\.\d+)/', $process->getOutput(), $matches) !== 1) {
            $this->components->warn(
                'Unable to determine the current PAM binary version. Please report this issue: https://github.com/push-in/pam/issues/new.',
            );

            return;
        }

        if (version_compare($matches[1], $this->requiredPamVersion, '<')) {
            $this->components->warn("Your PAM binary version (<fg=red>{$matches[1]}</>) may be incompatible with Octane. Please update it using [pam self-update].");
        }
    }
}
