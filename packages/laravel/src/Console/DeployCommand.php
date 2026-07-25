<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;
use Pam\Laravel\Enums\RemoteAction;
use Pam\Laravel\Services\AtomicDeployer;
use Pam\Laravel\Services\RemoteControlClient;
use Throwable;

final class DeployCommand extends Command
{
    protected $signature = 'pam:deploy {destination=production} {--rollback} {--local} {--release=}';
    protected $description = 'Deploy locally, to PAM Cloud, or through a Laravel Forge webhook';

    public function handle(AtomicDeployer $deployer, RemoteControlClient $remote): int
    {
        $argument = $this->argument('destination');
        $destination = is_string($argument) ? $argument : 'production';
        $local = $this->option('local') || is_dir($destination);
        try {
            if (!$local) {
                $parameters = [];
                $release = $this->option('release');
                if (is_string($release) && $release !== '') {
                    $parameters['release'] = $release;
                }
                $result = $remote->execute(
                    $this->option('rollback') ? RemoteAction::Rollback : RemoteAction::Deploy,
                    $destination,
                    $parameters,
                );
                $this->line((string) json_encode($result, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
                return self::SUCCESS;
            }
            if ($this->option('rollback')) {
                $active = $deployer->rollback();
                $this->info("Active release: {$active}");
                return self::SUCCESS;
            }
            if (!is_dir($destination)) {
                $this->error('Local deployment requires an existing release directory.');
                return self::INVALID;
            }
            $deployer->prepare($destination);
            $active = $deployer->activate($destination);
            if (!$deployer->ready()) {
                $deployer->rollback();
                $this->error('Readiness failed; the previous release was restored.');
                return self::FAILURE;
            }
            $this->info("Active release: {$active}");
        } catch (Throwable $exception) {
            $this->error($exception->getMessage());
            return self::FAILURE;
        }

        return self::SUCCESS;
    }
}
