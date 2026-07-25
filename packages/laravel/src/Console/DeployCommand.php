<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;
use Pam\Laravel\Services\AtomicDeployer;

final class DeployCommand extends Command
{
    protected $signature = 'pam:deploy {release?} {--rollback}';
    protected $description = 'Atomically activate or roll back a prepared Laravel release';

    public function handle(AtomicDeployer $deployer): int
    {
        if ($this->option('rollback')) {
            $active = $deployer->rollback();
            $this->info("Active release: {$active}");
            return self::SUCCESS;
        }

        $argument = $this->argument('release');
        $release = is_string($argument) ? $argument : '';
        if ($release === '') {
            $this->error('A release path is required unless --rollback is used.');
            return self::INVALID;
        }
        $deployer->prepare($release);
        $active = $deployer->activate($release);
        if (!$deployer->ready()) {
            $deployer->rollback();
            $this->error('Readiness failed; the previous release was restored.');
            return self::FAILURE;
        }
        $this->info("Active release: {$active}");

        return self::SUCCESS;
    }
}
