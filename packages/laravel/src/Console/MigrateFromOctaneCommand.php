<?php

declare(strict_types=1);

namespace Pam\Laravel\Console;

use Illuminate\Console\Command;

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
        $requires = array_merge($composer['require'] ?? [], $composer['require-dev'] ?? []);
        $findings = [];
        if (isset($requires['laravel/octane'])) {
            $findings[] = ['status' => 2, 'item' => 'dependency', 'message' => 'Remove laravel/octane after PAM validation.'];
        }
        foreach (['config/octane.php', 'app/Providers/OctaneServiceProvider.php'] as $path) {
            if (is_file(base_path($path))) {
                $findings[] = ['status' => 2, 'item' => $path, 'message' => 'Review Octane-specific lifecycle hooks and move cleanup to LifecycleHook.'];
            }
        }
        $findings[] = ['status' => 1, 'item' => 'server', 'message' => 'Replace `octane:start` with `pam serve` and use `pam.processes.json` for queues and scheduler.'];
        $findings[] = ['status' => 1, 'item' => 'verification', 'message' => 'Run pam:check-production, pam:leaks and production-like load tests.'];

        if ($this->option('json')) {
            $this->line((string) json_encode($findings, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
        } else {
            $this->table(['Status', 'Item', 'Recommendation'], array_map(
                fn (array $finding) => [$finding['status'] === 1 ? 'action' : 'review', $finding['item'], $finding['message']],
                $findings,
            ));
        }

        return self::SUCCESS;
    }
}
