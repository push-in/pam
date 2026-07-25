<?php

declare(strict_types=1);

namespace Pam\Laravel;

use Illuminate\Foundation\Application;
use Illuminate\Database\Events\QueryExecuted;
use Illuminate\Queue\Events\JobFailed;
use Illuminate\Queue\Events\JobProcessed;
use Illuminate\Queue\Events\JobProcessing;
use Illuminate\Routing\Router;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Event;
use Illuminate\Support\Facades\Route;
use Illuminate\Support\ServiceProvider;
use Pam\Laravel\Console\CapacityCommand;
use Pam\Laravel\Console\CheckProductionCommand;
use Pam\Laravel\Console\DeployCommand;
use Pam\Laravel\Console\HealthCommand;
use Pam\Laravel\Console\InstallCommand;
use Pam\Laravel\Console\LeaksCommand;
use Pam\Laravel\Console\MigrateFromOctaneCommand;
use Pam\Laravel\Console\SupervisorCommand;
use Pam\Laravel\Http\Middleware\GuardPersistentState;
use Pam\Laravel\Http\Middleware\ObserveRequest;
use Pam\Laravel\Enums\JobEventType;
use Pam\Laravel\Services\AtomicDeployer;
use Pam\Laravel\Services\HealthReporter;
use Pam\Laravel\Services\LifecycleManager;
use Pam\Laravel\Services\ObservabilityRegistry;
use Pam\Laravel\Services\ProcessSupervisor;
use Pam\Laravel\Services\ProductionChecker;
use Pam\Laravel\Services\StateGuard;

final class PamServiceProvider extends ServiceProvider
{
    public function register(): void
    {
        $this->mergeConfigFrom(__DIR__.'/../config/pam.php', 'pam');
        foreach ([
            LifecycleManager::class,
            ObservabilityRegistry::class,
            StateGuard::class,
        ] as $service) {
            $this->app->singleton($service);
        }
        $this->app->singleton(ProductionChecker::class);
        $this->app->singleton(HealthReporter::class);
        $this->app->singleton(ProcessSupervisor::class);
        $this->app->singleton(AtomicDeployer::class);
    }

    public function boot(Router $router): void
    {
        if ($this->app->runningInConsole()) {
            $this->commands([
                InstallCommand::class,
                CheckProductionCommand::class,
                HealthCommand::class,
                LeaksCommand::class,
                MigrateFromOctaneCommand::class,
                CapacityCommand::class,
                SupervisorCommand::class,
                DeployCommand::class,
            ]);
            $this->publishes([__DIR__.'/../config/pam.php' => config_path('pam.php')], 'pam-config');
            $this->publishes([
                __DIR__.'/../stubs/pam.processes.json' => base_path('pam.processes.json'),
                __DIR__.'/../stubs/docker-compose.pam.yml' => base_path('docker-compose.pam.yml'),
                __DIR__.'/../stubs/pam.service' => base_path('deploy/pam.service'),
                __DIR__.'/../stubs/kubernetes.yaml' => base_path('deploy/kubernetes.yaml'),
            ], 'pam-operations');
        }

        if ((bool) config('pam.state_guard.enabled', true)) {
            $router->pushMiddlewareToGroup('web', GuardPersistentState::class);
            $router->pushMiddlewareToGroup('api', GuardPersistentState::class);
        }
        if ((bool) config('pam.observability.enabled', true)) {
            $router->pushMiddlewareToGroup('web', ObserveRequest::class);
            $router->pushMiddlewareToGroup('api', ObserveRequest::class);
            DB::listen(function (QueryExecuted $query): void {
                $this->app->make(ObservabilityRegistry::class)->query($query->sql, $query->time);
            });
            Event::listen(JobProcessing::class, fn (JobProcessing $event) =>
                $this->app->make(ObservabilityRegistry::class)->job(JobEventType::Processing, $event->job->resolveName()));
            Event::listen(JobProcessed::class, fn (JobProcessed $event) =>
                $this->app->make(ObservabilityRegistry::class)->job(JobEventType::Processed, $event->job->resolveName()));
            Event::listen(JobFailed::class, fn (JobFailed $event) =>
                $this->app->make(ObservabilityRegistry::class)->job(JobEventType::Failed, $event->job->resolveName()));
        }
        $this->registerHealthRoutes();
    }

    private function registerHealthRoutes(): void
    {
        if (!(bool) config('pam.health.enabled', true)
            || ($this->app instanceof Application && $this->app->routesAreCached())) {
            return;
        }
        Route::get((string) config('pam.health.path'), function (HealthReporter $reporter) {
            $report = $reporter->report();
            return response()->json($report, $report['ok'] ? 200 : 503);
        })->name('pam.health');
        Route::get((string) config('pam.health.metrics_path'), function (ObservabilityRegistry $registry) {
            $token = (string) config('pam.health.token', '');
            if ($token !== '' && !hash_equals($token, (string) request()->bearerToken())) {
                abort(403);
            }
            return response()->json($registry->snapshot());
        })->name('pam.metrics');
    }
}
