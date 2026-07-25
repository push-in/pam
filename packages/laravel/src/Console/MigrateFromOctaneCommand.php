<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;
use Pam\Laravel\Enums\MigrationFindingStatus;

final class MigrateFromOctaneCommand extends Command
{
    protected $signature = 'pam:migrate-from-octane {--json}';
    protected $description = 'Audit an Octane application for migration to the PAM runtime';

    public function handle(): int
    {
        $composer = json_decode((string) file_get_contents(base_path('composer.json')), true);
        if (!is_array($composer)) {
            $this->error('composer.json is invalid.');
            return self::FAILURE;
        }
        $require = is_array($composer['require'] ?? null) ? $composer['require'] : [];
        $requireDev = is_array($composer['require-dev'] ?? null) ? $composer['require-dev'] : [];
        $requires = array_merge($require, $requireDev);
        $findings = [];
        if (isset($requires['laravel/octane'])) {
            $findings[] = ['status' => MigrationFindingStatus::Review->value, 'item' => 'dependency', 'message' => 'Remove laravel/octane after PAM validation.'];
        }
        foreach (['config/octane.php', 'app/Providers/OctaneServiceProvider.php'] as $path) {
            if (is_file(base_path($path))) {
                $findings[] = ['status' => MigrationFindingStatus::Review->value, 'item' => $path, 'message' => 'Review Octane-specific lifecycle hooks and move cleanup to LifecycleHook.'];
            }
        }
        $findings[] = ['status' => MigrationFindingStatus::Action->value, 'item' => 'server', 'message' => 'Replace `octane:start` with `pam start pam.php --workers N`; keep queues and the scheduler in `pam.processes.json`.'];
        $findings[] = ['status' => MigrationFindingStatus::Action->value, 'item' => 'verification', 'message' => 'Run pam:check-production, pam:leaks and production-like load tests.'];

        if ($this->option('json')) {
            $this->line((string) json_encode($findings, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
        } else {
            $this->table(['Status', 'Item', 'Recommendation'], array_map(
                fn (array $finding) => [
                    MigrationFindingStatus::from($finding['status'])->label(),
                    $finding['item'],
                    $finding['message'],
                ],
                $findings,
            ));
        }

        return self::SUCCESS;
    }
}
